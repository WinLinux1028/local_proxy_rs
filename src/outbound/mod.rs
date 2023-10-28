pub mod layer;

mod http;
mod raw;
mod socks4;

pub use http::HttpProxy;
pub use raw::Raw;
pub use socks4::Socks4Proxy;

use crate::{
    outbound::layer::Layer,
    utils::{self},
    Connection, Error,
};

use async_trait::async_trait;
use hyper::{upgrade::OnUpgrade, Body, Request, Response, StatusCode};

#[async_trait]
pub trait ProxyOutBound: Send + Sync {
    async fn connect(
        &self,
        proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        hostname: &str,
        port: u16,
    ) -> Result<Connection, Error>;

    async fn http_proxy(
        &self,
        proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        scheme: &str,
        request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        self.http_proxy_(proxies, scheme, request).await
    }
}

#[async_trait]
pub trait ProxyOutBoundDefaultMethods: ProxyOutBound {
    async fn http_proxy_(
        &self,
        proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        scheme: &str,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        let mut host_header = request
            .headers()
            .get("host")
            .ok_or("")?
            .to_str()?
            .split(':');
        let hostname = host_header.next().ok_or("")?;
        let port: u16 = match host_header.next() {
            Some(port) => port.parse()?,
            None => match scheme {
                "http" => 80,
                "https" => 443,
                _ => return Err("".into()),
            },
        };

        let mut server = self.connect(proxies, hostname, port).await?;
        if scheme == "https" {
            server = layer::TlsClient::new().wrap(server, hostname, port).await?;
        }
        let (mut sender, conn) = hyper::client::conn::handshake(server).await?;
        tokio::spawn(conn);

        let client = hyper::upgrade::on(&mut request);
        let mut response = sender.send_request(request).await?;
        let server = hyper::upgrade::on(&mut response);
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            tokio::spawn(Self::proxy_upgrade(client, server));
        }

        Ok(response)
    }

    async fn proxy_upgrade(client: OnUpgrade, server: OnUpgrade) -> Result<(), Error> {
        let (client, server) = tokio::join!(client, server);
        utils::copy_bidirectional(client?, server?).await;
        Ok(())
    }
}
impl<P> ProxyOutBoundDefaultMethods for P where P: ProxyOutBound + ?Sized {}
