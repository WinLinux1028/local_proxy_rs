mod fragment;
mod tls;

pub use fragment::Fragment;
pub use tls::TlsClient;

use super::{ProxyOutBound, ProxyOutBoundDefaultMethods};
use crate::{
    inbound::http::http_proxy::RequestConfig,
    outbound::ProxyStack,
    utils::{Body, SocketAddr},
    Connection, Error,
};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use hyper::{Request, Response};

#[async_trait]
pub trait Layer: Sync + Send {
    async fn wrap<RW>(&self, stream: RW, addr: &SocketAddr) -> Result<Connection, Error>
    where
        RW: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    fn is_http_passthrough(&self) -> bool {
        false
    }
}

#[async_trait]
impl<L> ProxyOutBound for L
where
    L: Layer,
{
    async fn connect(
        &self,
        mut proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let server = proxies.next().ok_or("")?.connect(proxies, addr).await?;
        self.wrap(server, addr).await
    }

    async fn http_proxy(
        &self,
        mut proxies: ProxyStack<'_>,
        scheme: &str,
        req_conf: &RequestConfig,
        request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        if self.is_http_passthrough() {
            proxies
                .next()
                .ok_or("")?
                .http_proxy(proxies, scheme, req_conf, request)
                .await
        } else {
            self.http_proxy_(proxies, scheme, req_conf, request).await
        }
    }
}
