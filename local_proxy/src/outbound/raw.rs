use super::ProxyOutBound;
use crate::{outbound::ProxyStack, utils::SocketAddr, Connection, Error};

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
        mut proxies: ProxyStack<'_>,
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
