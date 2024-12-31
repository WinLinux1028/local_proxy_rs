use super::ProxyOutBound;
use crate::{
    config::ProxyConfig,
    outbound::ProxyStack,
    utils::{HostName, SocketAddr},
    Connection, Error,
};

use std::{net::Ipv4Addr, str::FromStr};

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Socks4Proxy {
    addr: SocketAddr,
    auth: Option<String>,
}

impl Socks4Proxy {
    pub fn new(conf: &ProxyConfig) -> Result<Self, Error> {
        let mut auth = String::new();

        if let Some(user) = &conf.user {
            auth += user;
        }
        if let Some(password) = &conf.password {
            auth += ":";
            auth += password;
        }

        if auth.contains('\0') {
            return Err("".into());
        }
        let auth = if !auth.is_empty() { Some(auth) } else { None };

        Ok(Self {
            addr: SocketAddr::from_str(&conf.server)?,
            auth,
        })
    }
}

#[async_trait]
impl ProxyOutBound for Socks4Proxy {
    async fn connect(
        &self,
        mut proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let ip;
        let mut hostname = None;
        match &addr.hostname {
            HostName::V4(v4) => {
                let v4_integer = u32::from_be_bytes(v4.octets());
                if v4_integer & 0xFFFFFF00 == 0 && v4_integer & 0xFF != 0 {
                    return Err("".into());
                }
                ip = *v4
            }
            HostName::V6(v6) => {
                ip = Ipv4Addr::new(0, 0, 0, 1);
                hostname = Some(v6.to_string());
            }
            HostName::Domain(domain) => {
                ip = Ipv4Addr::new(0, 0, 0, 1);
                if domain.contains('\0') {
                    return Err("".into());
                }
                hostname = Some(domain.clone());
            }
        }

        let mut server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;

        server.write_all(&[4, 1]).await?;
        server.write_all(&addr.port.to_be_bytes()).await?;
        server.write_all(&ip.octets()).await?;
        if let Some(auth) = &self.auth {
            server.write_all(auth.as_bytes()).await?
        }
        server.write_all(b"\0").await?;
        if let Some(hostname) = &hostname {
            server.write_all(hostname.as_bytes()).await?;
            server.write_all(b"\0").await?;
        }
        server.flush().await?;

        if server.read_u8().await? != 0 {
            return Err("".into());
        }
        if server.read_u8().await? != 90 {
            return Err("".into());
        }
        let mut buf = [0; 6];
        server.read_exact(&mut buf).await?;

        Ok(Box::new(server))
    }
}
