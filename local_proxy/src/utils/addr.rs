use crate::{utils::doh_query, Error};

use dns_parser::{QueryClass, QueryType, RData};
use std::{
    fmt::{Display, Write},
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

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

        if let Some(host) = host.rsplit_once(':') {
            hostname = host.0;
            port = Some(host.1.parse()?);
        } else {
            hostname = host;
        }

        let hostname = HostName::from_str(hostname)?;
        Ok((hostname, port))
    }
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let HostName::V6(_) = &self.hostname {
            f.write_char('[')?;
            f.write_str(&self.hostname.to_string())?;
            f.write_str("]:")?;
            f.write_str(&self.port.to_string())
        } else {
            f.write_str(&self.hostname.to_string())?;
            f.write_char(':')?;
            f.write_str(&self.port.to_string())
        }
    }
}

impl FromStr for SocketAddr {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (hostname, port) = s.rsplit_once(':').ok_or("")?;

        Ok(Self {
            hostname: HostName::from_str(hostname)?,
            port: port.parse()?,
        })
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
    pub async fn dns_resolve(&self, qtype: QueryType) -> Result<Option<Self>, Error> {
        let domain = match self {
            Self::Domain(domain) => domain,
            _ => return Err("".into()),
        };

        let mut query = dns_parser::Builder::new_query(0xabcd, true);
        query.add_question(domain, false, qtype, QueryClass::IN);
        let query = query.build().map_err(|_| "")?;

        let result = doh_query(query).await?;
        let response_body = dns_parser::Packet::parse(&result)?;

        for answer in response_body.answers {
            if answer.cls != dns_parser::Class::IN {
                continue;
            }
            match answer.data {
                RData::A(addr) => {
                    return Ok(Some(addr.0.into()));
                }
                RData::AAAA(addr) => {
                    return Ok(Some(addr.0.into()));
                }
                _ => continue,
            }
        }
        Ok(None)
    }

    pub fn to_string_url_style(&self) -> String {
        match self {
            Self::V6(v6) => format!("[{}]", v6),
            _ => self.to_string(),
        }
    }

    pub fn is_ipaddr(&self) -> bool {
        match self {
            Self::V4(_) => true,
            Self::V6(_) => true,
            Self::Domain(_) => false,
        }
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
    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        if s.starts_with('[') && s.ends_with(']') {
            s = &s[1..(s.len() - 1)];
        }

        if let Ok(ip) = std::net::IpAddr::from_str(s) {
            Ok(ip.into())
        } else {
            Ok(Self::Domain(s.to_string()))
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
