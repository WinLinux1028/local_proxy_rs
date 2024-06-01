pub mod layer;

mod http;
mod raw;
mod socks4;
mod socks5;

use dns_parser::QueryType;
use dyn_clone::DynClone;
pub use http::HttpProxy;
use hyper_util::rt::TokioIo;
pub use raw::Raw;
pub use socks4::Socks4Proxy;
pub use socks5::Socks5Proxy;

use crate::{
    outbound::layer::Layer,
    utils::{self, Body, SocketAddr},
    Connection, Error,
};

use async_trait::async_trait;
use hyper::{upgrade::OnUpgrade, Request, Response, StatusCode};

#[async_trait]
pub trait ProxyOutBound: Send + Sync {
    async fn connect(
        &self,
        proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error>;

    async fn http_proxy(
        &self,
        proxies: ProxyStack<'_>,
        scheme: &str,
        use_doh: bool,
        request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        self.http_proxy_(proxies, scheme, use_doh, request).await
    }
}

#[async_trait]
pub trait ProxyOutBoundDefaultMethods: ProxyOutBound {
    async fn happy_eyeballs(
        &self,
        proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let conn;
        tokio::select! {
            Ok(conn_) = async {
                let ip = addr.hostname.dns_resolve(QueryType::AAAA).await?;
                let addr = SocketAddr::new(ip, addr.port);
                let proxies = dyn_clone::clone_box(&*proxies);
                let conn = self.connect(proxies, &addr).await?;
                Ok::<_, Error>(conn)
            } => conn = conn_,
            Ok(conn_) = async {
                let ip = addr.hostname.dns_resolve(QueryType::A).await?;
                let addr = SocketAddr::new(ip, addr.port);
                let proxies = dyn_clone::clone_box(&*proxies);
                let conn = self.connect(proxies, &addr).await?;
                Ok::<_, Error>(conn)
            } => conn = conn_,
            else => {
                let proxies = dyn_clone::clone_box(&*proxies);
                conn = self
                    .connect(proxies, addr)
                    .await?;
            }
        }

        Ok(conn)
    }

    async fn http_proxy_(
        &self,
        proxies: ProxyStack<'_>,
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
            server = self.happy_eyeballs(proxies, &addr).await?;
        } else {
            server = self.connect(proxies, &addr).await?;
        }

        if scheme == "https" {
            server = layer::TlsClient::new().wrap(server, &addr).await?;
        }
        let server = TokioIo::new(server);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(server).await?;
        tokio::spawn(conn.with_upgrades());

        let client = hyper::upgrade::on(&mut request);
        let mut response = sender.send_request(request).await?;
        let server = hyper::upgrade::on(&mut response);
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            tokio::spawn(Self::proxy_upgrade(client, server));
        }

        Ok(Body::convert_response(response))
    }

    async fn proxy_upgrade(client: OnUpgrade, server: OnUpgrade) -> Result<(), Error> {
        let (client, server) = tokio::join!(client, server);
        utils::copy_bidirectional(TokioIo::new(client?), TokioIo::new(server?)).await;
        Ok(())
    }
}
impl<P> ProxyOutBoundDefaultMethods for P where P: ProxyOutBound + ?Sized {}

pub type ProxyStack<'a> = Box<dyn ClonableIterator<Item = &'a dyn ProxyOutBound> + Send + Sync>;
pub trait ClonableIterator: Iterator + DynClone {}
impl<T: Iterator + DynClone> ClonableIterator for T {}
