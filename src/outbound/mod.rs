mod http;
mod raw;

pub use http::HttpProxy;
pub use raw::Raw;

use crate::{utils::UnSplit, Error};

use hyper::{Body, Request, Response};
use tokio::io::{AsyncBufRead, AsyncWrite};

use async_trait::async_trait;

#[async_trait]
pub trait ProxyOutBound: std::fmt::Debug + Unpin + Sync + Send {
    async fn connect(
        &self,
        addr: &str,
        port: u16,
    ) -> Result<
        UnSplit<
            Box<dyn AsyncBufRead + Unpin + Sync + Send>,
            Box<dyn AsyncWrite + Unpin + Sync + Send>,
        >,
        Error,
    >;

    async fn http_proxy(&self, request: Request<Body>) -> Result<Response<Body>, Error> {
        let uri = request.uri();
        let server = self
            .connect(uri.host().ok_or("")?, uri.port_u16().unwrap_or(80))
            .await?;

        let (mut sender, conn) = hyper::client::conn::handshake(server).await?;
        let conn = tokio::spawn(conn);

        let result = sender.send_request(request).await?;
        conn.abort();

        Ok(result)
    }
}
