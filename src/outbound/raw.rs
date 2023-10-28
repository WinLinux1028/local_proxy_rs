use super::ProxyOutBound;
use crate::{Connection, Error};

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
        hostname: &str,
        port: u16,
    ) -> Result<Connection, Error> {
        if let Some(proxy) = proxies.next() {
            return proxy.connect(proxies, hostname, port).await;
        }

        let server = TcpStream::connect(format!("{}:{}", hostname, port)).await?;
        server.set_nodelay(true)?;

        Ok(Box::new(server))
    }
}
