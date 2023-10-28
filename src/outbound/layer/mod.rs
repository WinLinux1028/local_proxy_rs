mod tls;

pub use tls::TlsClient;

use super::ProxyOutBound;
use crate::{Connection, Error};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

#[async_trait]
pub trait Layer: Sync + Send {
    async fn wrap<RW>(&self, stream: RW, hostname: &str, port: u16) -> Result<Connection, Error>
    where
        RW: AsyncRead + AsyncWrite + Unpin + Send + 'static;
}

#[async_trait]
impl<L> ProxyOutBound for L
where
    L: Layer,
{
    async fn connect(
        &self,
        mut proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        hostname: &str,
        port: u16,
    ) -> Result<Connection, Error> {
        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, hostname, port)
            .await?;
        self.wrap(server, hostname, port).await
    }
}
