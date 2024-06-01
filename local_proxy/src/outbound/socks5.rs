use super::ProxyOutBound;
use crate::{
    config::ProxyConfig,
    outbound::ProxyStack,
    utils::{HostName, SocketAddr},
    Connection, Error,
};

use std::str::FromStr;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Socks5Proxy {
    addr: SocketAddr,
    user: String,
    password: String,
}

impl Socks5Proxy {
    pub fn new(conf: &ProxyConfig) -> Result<Self, Error> {
        let user = match &conf.user {
            Some(s) => s.clone(),
            None => String::new(),
        };
        if user.len() > 255 {
            return Err("".into());
        }

        let password = match &conf.password {
            Some(s) => s.clone(),
            None => String::new(),
        };
        if password.len() > 255 {
            return Err("".into());
        }

        Ok(Self {
            addr: SocketAddr::from_str(&conf.server)?,
            user,
            password,
        })
    }
}

#[async_trait]
impl ProxyOutBound for Socks5Proxy {
    async fn connect(
        &self,
        mut proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let mut server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;

        server.write_all(&[5, 2, 0, 2]).await?;
        server.flush().await?;

        if server.read_u8().await? != 5 {
            return Err("".into());
        }
        match server.read_u8().await? {
            0 => {}
            2 => {
                server.write_all(&[1, self.user.len().try_into()?]).await?;
                server.write_all(self.user.as_bytes()).await?;
                server.write_all(&[self.password.len().try_into()?]).await?;
                server.write_all(self.password.as_bytes()).await?;
                server.flush().await?;

                if server.read_u8().await? != 1 {
                    return Err("".into());
                }
                if server.read_u8().await? != 0 {
                    return Err("".into());
                }
            }
            _ => return Err("".into()),
        }

        server.write_all(&[5, 1, 0]).await?;
        match &addr.hostname {
            HostName::V4(v4) => {
                server.write_all(&[1]).await?;
                server.write_all(&v4.octets()).await?;
            }
            HostName::V6(v6) => {
                server.write_all(&[4]).await?;
                server.write_all(&v6.octets()).await?;
            }
            HostName::Domain(domain) => {
                server.write_all(&[3, domain.len().try_into()?]).await?;
                server.write_all(domain.as_bytes()).await?;
            }
        }
        server.write_all(&addr.port.to_be_bytes()).await?;
        server.flush().await?;

        if server.read_u8().await? != 5 {
            return Err("".into());
        }
        if server.read_u8().await? != 0 {
            return Err("".into());
        }
        if server.read_u8().await? != 0 {
            return Err("".into());
        }
        match server.read_u8().await? {
            1 => {
                server.read_u32().await?;
            }
            3 => {
                let mut buf = vec![0; server.read_u8().await?.into()];
                server.read_exact(&mut buf).await?;
            }
            4 => {
                server.read_u128().await?;
            }
            _ => return Err("".into()),
        }
        server.read_u16().await?;

        Ok(Box::new(server))
    }
}
