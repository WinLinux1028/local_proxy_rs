use super::ProxyOutBound;
use crate::{
    config::ProxyConfig,
    utils::{HostName, SocketAddr},
    Connection, Error,
};

use std::str::FromStr;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Socks4Proxy {
    addr: SocketAddr,
    auth: Option<String>,
}

impl Socks4Proxy {
    pub fn new(conf: &ProxyConfig) -> Result<Self, Error> {
        let mut auth = None;
        if let Some(user) = &conf.user {
            let mut auth_ = user.clone();
            if let Some(password) = &conf.password {
                auth_.push(':');
                auth_.push_str(password);
            }
            if auth_.contains('\0') {
                return Err("".into());
            }
            auth = Some(auth_);
        }

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
        mut proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let ip;
        let mut hostname = None;
        match &addr.hostname {
            HostName::V4(v4) => ip = *v4,
            HostName::Domain(domain) => {
                ip = "0.0.0.1".parse()?;
                if domain.contains('\0') {
                    return Err("".into());
                }
                hostname = Some(domain);
            }
            HostName::V6(_) => return Err("".into()),
        }

        let ip_octet = ip.octets();
        if hostname.is_none()
            && ip_octet[0] == 0
            && ip_octet[1] == 0
            && ip_octet[2] == 0
            && ip_octet[3] != 0
        {
            return Err("".into());
        }

        let mut server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;

        server.write_all(&[4, 1]).await?;
        server.write_all(&addr.port.to_be_bytes()).await?;
        server.write_all(&ip_octet).await?;
        if let Some(auth) = &self.auth {
            server.write_all(auth.as_bytes()).await?
        }
        server.write_all(b"\0").await?;
        if let Some(hostname) = hostname {
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
