use super::ProxyOutBound;
use crate::{config::ProxyConfig, utils::SocketAddr, Connection, Error};

use std::str::FromStr;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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
        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;
        let mut server = BufReader::new(server);

        server.write_all(&[4, 1]).await?;

        Ok(Box::new(server))
    }
}
