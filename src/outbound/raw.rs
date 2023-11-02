use super::ProxyOutBound;
use crate::{utils::SocketAddr, Connection, Error};

use tokio::net::TcpStream;

use async_trait::async_trait;

pub struct Raw();

impl Raw {
    pub fn new() -> Self {
        Self()
    }
}

#[async_trait]
impl ProxyOutBound for Raw {
    async fn connect(
        &self,
        mut proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        if let Some(proxy) = proxies.next() {
            return proxy.connect(proxies, addr).await;
        }

        let server = TcpStream::connect(addr.to_string()).await?;
        server.set_nodelay(true)?;

        Ok(Box::new(server))
    }
}
