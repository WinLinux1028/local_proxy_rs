use crate::{utils::doh_query, Connection, DnsCacheState, Error, PROXY};

use std::{
    fmt::{Display, Write},
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
    time::Duration,
};

use dns_parser::{QueryClass, QueryType, RData};
use hyper::Uri;

#[derive(Clone)]
pub struct SocketAddr {
    pub hostname: HostName,
    pub port: u16,
}

impl SocketAddr {
    pub fn new(hostname: HostName, port: u16) -> Self {
        Self { hostname, port }
    }

    pub fn parse_host_header(host: &str) -> Result<(HostName, Option<u16>), Error> {
        let hostname;
        let mut port = None;
        if host.starts_with('[') && host.ends_with(']') {
            hostname = &host[1..(host.len() - 1)];
        } else if let Some(host) = host.rsplit_once(':') {
            hostname = host.0;
            port = Some(host.1.parse()?);
        } else {
            hostname = host;
        }

        let hostname = HostName::from_str(hostname)?;
        Ok((hostname, port))
    }

    pub async fn happy_eyeballs(&self) -> Result<Connection, Error> {
        let proxy = PROXY.get().ok_or("")?;

        let conn;
        tokio::select! {
            Ok(conn_) = async {
                let ip = self.hostname.dns_resolve(QueryType::AAAA).await?;
                let addr = SocketAddr::new(ip, self.port);
                let mut proxies = proxy.proxy_stack.iter().rev();
                let conn = proxies.next().ok_or("")?.connect(Box::new(proxies), &addr).await?;
                Ok::<_, Error>(conn)
            } => conn = conn_,
            Ok(conn_) = async {
                let ip = self.hostname.dns_resolve(QueryType::A).await?;
                let addr = SocketAddr::new(ip, self.port);
                let mut proxies = proxy.proxy_stack.iter().rev();
                let conn = proxies.next().ok_or("")?.connect(Box::new(proxies), &addr).await?;
                Ok::<_, Error>(conn)
            } => conn = conn_,
            else => {
                let mut proxies = proxy.proxy_stack.iter().rev();
                conn = proxies
                    .next()
                    .ok_or("")?
                    .connect(Box::new(proxies), self)
                    .await?;
            }
        }

        Ok(conn)
    }
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hostname = self.hostname.to_string();
        if let HostName::V6(_) = &self.hostname {
            f.write_char('[')?;
            f.write_str(&hostname)?;
            f.write_str("]:")?;
            f.write_str(&self.port.to_string())
        } else {
            f.write_str(&hostname)?;
            f.write_char(':')?;
            f.write_str(&self.port.to_string())
        }
    }
}

impl FromStr for SocketAddr {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = std::net::SocketAddr::from_str(s) {
            Ok(addr.into())
        } else {
            let addr: Vec<&str> = s.split(':').collect();
            if addr.len() != 2 {
                return Err("".into());
            }

            Ok(Self {
                hostname: HostName::Domain(addr[0].to_string()),
                port: addr[1].parse()?,
            })
        }
    }
}

impl From<std::net::SocketAddr> for SocketAddr {
    fn from(value: std::net::SocketAddr) -> Self {
        Self {
            hostname: value.ip().into(),
            port: value.port(),
        }
    }
}

impl TryFrom<&SocketAddr> for std::net::SocketAddr {
    type Error = Error;
    fn try_from(value: &SocketAddr) -> Result<Self, Self::Error> {
        Ok(Self::new((&value.hostname).try_into()?, value.port))
    }
}

impl TryFrom<&SocketAddr> for std::net::SocketAddrV4 {
    type Error = Error;
    fn try_from(value: &SocketAddr) -> Result<Self, Self::Error> {
        Ok(Self::new((&value.hostname).try_into()?, value.port))
    }
}

impl TryFrom<&SocketAddr> for std::net::SocketAddrV6 {
    type Error = Error;
    fn try_from(value: &SocketAddr) -> Result<Self, Self::Error> {
        Ok(Self::new((&value.hostname).try_into()?, value.port, 0, 0))
    }
}

#[derive(Clone)]
pub enum HostName {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
    Domain(String),
}

impl HostName {
    pub async fn dns_resolve(&self, qtype: QueryType) -> Result<Self, Error> {
        let domain = match self {
            Self::Domain(domain) => domain,
            _ => return Err("".into()),
        };

        let proxy = PROXY.get().ok_or("")?;
        let uri = Uri::from_str(proxy.config.doh_endpoint.as_ref().ok_or("")?)?;

        let dns_cache = proxy.dns_cache.read().await;
        if let Some(cache_content) = dns_cache.get(domain) {
            if qtype == QueryType::A {
                match cache_content.0 {
                    DnsCacheState::Some(s) => return Ok(s.into()),
                    DnsCacheState::Fail => return Err("".into()),
                    DnsCacheState::None => {}
                }
            } else if qtype == QueryType::AAAA {
                match cache_content.1 {
                    DnsCacheState::Some(s) => return Ok(s.into()),
                    DnsCacheState::Fail => return Err("".into()),
                    DnsCacheState::None => {}
                }
            } else {
                return Err("".into());
            }
        }
        drop(dns_cache);

        let mut query = dns_parser::Builder::new_query(0xabcd, true);
        query.add_question(domain, false, qtype, QueryClass::IN);
        let query = query.build().map_err(|_| "")?;

        let result = doh_query(&uri, query).await?;
        let response_body = dns_parser::Packet::parse(&result)?;

        let mut dns_cache = proxy.dns_cache.write().await;
        if !dns_cache.contains_key(domain) {
            dns_cache.insert(
                domain.clone(),
                (DnsCacheState::None, DnsCacheState::None),
                Duration::from_secs(7200),
            );
        }
        let cache_content = dns_cache.get_mut(domain).ok_or("")?;

        for answer in response_body.answers {
            if answer.cls != dns_parser::Class::IN {
                continue;
            }
            match answer.data {
                RData::A(addr) => {
                    cache_content.0 = DnsCacheState::Some(addr.0);
                    return Ok(addr.0.into());
                }
                RData::AAAA(addr) => {
                    cache_content.1 = DnsCacheState::Some(addr.0);
                    return Ok(addr.0.into());
                }
                _ => continue,
            }
        }

        if qtype == QueryType::A {
            cache_content.0 = DnsCacheState::Fail;
        } else if qtype == QueryType::AAAA {
            cache_content.1 = DnsCacheState::Fail;
        }
        Err("".into())
    }
}

impl Display for HostName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V4(v4) => f.write_str(&v4.to_string()),
            Self::V6(v6) => f.write_str(&v6.to_string()),
            Self::Domain(domain) => f.write_str(domain),
        }
    }
}

impl FromStr for HostName {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(ip) = std::net::IpAddr::from_str(s) {
            Ok(ip.into())
        } else {
            Ok(Self::Domain(idna::domain_to_ascii_strict(s)?))
        }
    }
}

impl From<std::net::IpAddr> for HostName {
    fn from(value: std::net::IpAddr) -> Self {
        match value {
            std::net::IpAddr::V4(v4) => v4.into(),
            std::net::IpAddr::V6(v6) => v6.into(),
        }
    }
}

impl From<std::net::Ipv4Addr> for HostName {
    fn from(value: std::net::Ipv4Addr) -> Self {
        Self::V4(value)
    }
}

impl From<std::net::Ipv6Addr> for HostName {
    fn from(value: std::net::Ipv6Addr) -> Self {
        match value.to_ipv4_mapped() {
            Some(v4) => Self::V4(v4),
            None => Self::V6(value),
        }
    }
}

impl TryFrom<&HostName> for std::net::IpAddr {
    type Error = Error;
    fn try_from(value: &HostName) -> Result<Self, Self::Error> {
        match value {
            HostName::V4(v4) => Ok(Self::V4(*v4)),
            HostName::V6(v6) => Ok(Self::V6(*v6)),
            _ => Err("".into()),
        }
    }
}

impl TryFrom<&HostName> for std::net::Ipv4Addr {
    type Error = Error;
    fn try_from(value: &HostName) -> Result<Self, Self::Error> {
        match value {
            HostName::V4(v4) => Ok(*v4),
            _ => Err("".into()),
        }
    }
}

impl TryFrom<&HostName> for std::net::Ipv6Addr {
    type Error = Error;
    fn try_from(value: &HostName) -> Result<Self, Self::Error> {
        match value {
            HostName::V6(v6) => Ok(*v6),
            _ => Err("".into()),
        }
    }
}
