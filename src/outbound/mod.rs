mod http;
mod raw;

pub use http::HttpProxy;
pub use raw::Raw;

use crate::{
    utils::{self, UnSplit},
    Error,
};

use hyper::{upgrade::OnUpgrade, Body, Request, Response, StatusCode};
use tokio::io::{AsyncBufRead, AsyncWrite, BufReader};

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

    async fn http_proxy(
        &self,
        scheme: &str,
        addr: &str,
        port: u16,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        let server = self.connect(addr, port).await?;

        let mut sender;
        if scheme == "http" {
            let (sender_, conn) = hyper::client::conn::handshake(server).await?;
            tokio::spawn(conn);
            sender = sender_;
        } else if scheme == "https" {
            let (sender_, conn) = hyper::client::conn::handshake(server).await?;
            tokio::spawn(conn);
            sender = sender_;
        } else {
            return Err("".into());
        }

        let client = hyper::upgrade::on(&mut request);
        let mut response = sender.send_request(request).await?;
        let server = hyper::upgrade::on(&mut response);
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            tokio::spawn(proxy_upgrade(client, server));
        }

        Ok(response)
    }
}

async fn proxy_upgrade(client: OnUpgrade, server: OnUpgrade) -> Result<(), Error> {
    let (client, server) = tokio::join!(client, server);
    let client = tokio::io::split(client?);
    let client = unsafe { UnSplit::new(BufReader::new(client.0), client.1) };
    let server = tokio::io::split(server?);
    let server = unsafe { UnSplit::new(BufReader::new(server.0), server.1) };

    utils::copy_bidirectional(client, server).await;
    Ok(())
}
