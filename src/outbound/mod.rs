pub mod layer;

mod http;
mod raw;
mod socks4;

pub use http::HttpProxy;
pub use raw::Raw;
pub use socks4::Socks4Proxy;

use crate::{
    outbound::layer::Layer,
    utils::{self, SocketAddr},
    Connection, Error,
};

use async_trait::async_trait;
use hyper::{upgrade::OnUpgrade, Body, Request, Response, StatusCode};

#[async_trait]
pub trait ProxyOutBound: Send + Sync {
    async fn connect(
        &self,
        proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error>;

    async fn http_proxy(
        &self,
        proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        scheme: &str,
        use_doh: bool,
        request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        self.http_proxy_(proxies, scheme, use_doh, request).await
    }
}

#[async_trait]
pub trait ProxyOutBoundDefaultMethods: ProxyOutBound {
    async fn http_proxy_(
        &self,
        proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        scheme: &str,
        use_doh: bool,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        let host = request.headers().get("host").ok_or("")?.to_str()?;
        let (hostname, port) = SocketAddr::parse_host_header(host)?;
        let port = match port {
            Some(s) => s,
            None => match scheme {
                "http" => 80,
                "https" => 443,
                _ => return Err("".into()),
            },
        };
        let addr = SocketAddr::new(hostname, port);

        let mut server;
        if use_doh {
            server = addr.happy_eyeballs().await?;
        } else {
            server = self.connect(proxies, &addr).await?;
        }

        if scheme == "https" {
            server = layer::TlsClient::new().wrap(server, &addr).await?;
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
