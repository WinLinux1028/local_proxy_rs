mod tls;

pub use tls::TlsClient;

use super::ProxyOutBound;
use crate::{utils::SocketAddr, Connection, Error};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

#[async_trait]
pub trait Layer: Sync + Send {
    async fn wrap<RW>(&self, stream: RW, addr: &SocketAddr) -> Result<Connection, Error>
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
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let server = proxies.next().ok_or("")?.connect(proxies, addr).await?;
        self.wrap(server, addr).await
    }
}
