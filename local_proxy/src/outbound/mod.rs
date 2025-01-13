pub mod layer;

mod http;
mod raw;
mod socks4;
mod socks5;

pub use http::HttpProxy;
pub use raw::Raw;
pub use socks4::Socks4Proxy;
pub use socks5::Socks5Proxy;

use crate::{
    inbound::http::http_proxy::RequestConfig,
    outbound::layer::Layer,
    utils::{Body, SocketAddr},
    Connection, Error, PROXY,
};

use async_trait::async_trait;
use dns_parser::QueryType;
use dyn_clone::DynClone;
use hyper::{upgrade::OnUpgrade, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io;

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
        req_conf: &RequestConfig,
        request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        self.http_proxy_(proxies, scheme, req_conf, request).await
    }
}

#[async_trait]
pub trait ProxyOutBoundDefaultMethods: ProxyOutBound {
    async fn happy_eyeballs(
        &self,
        proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let proxy = PROXY.get().ok_or("")?;
        if proxy.config.doh.is_none() || addr.hostname.is_ipaddr() {
            return self.connect(proxies, addr).await;
        }

        let mut doh_failed_v6 = true;
        let mut doh_failed_v4 = true;
        let conn;
        tokio::select! {
            Ok(conn_) = async {
                let ip = addr.hostname.dns_resolve(QueryType::AAAA).await?;
                doh_failed_v6 = false;
                let addr = SocketAddr::new(ip.ok_or("")?, addr.port);
                let proxies = dyn_clone::clone_box(&*proxies);
                let conn = self.connect(proxies, &addr).await?;
                Ok::<_, Error>(conn)
            } => conn = conn_,
            Ok(conn_) = async {
                let ip = addr.hostname.dns_resolve(QueryType::A).await?;
                doh_failed_v4 = false;
                let addr = SocketAddr::new(ip.ok_or("")?, addr.port);
                let proxies = dyn_clone::clone_box(&*proxies);
                let conn = self.connect(proxies, &addr).await?;
                Ok::<_, Error>(conn)
            } => conn = conn_,
            else => {
                if doh_failed_v6 || doh_failed_v4 {
                    eprintln!("[Warning] DoH failed and fallbacked to DoH disable.");
                }
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
        req_conf: &RequestConfig,
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
        let fake_addr = SocketAddr::new(
            req_conf.fake_host.clone().unwrap_or(addr.hostname.clone()),
            addr.port,
        );

        let mut server;
        if req_conf.doh {
            server = self.happy_eyeballs(proxies, &fake_addr).await?;
        } else {
            server = self.connect(proxies, &fake_addr).await?;
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
        let mut client = TokioIo::new(client?);
        let mut server = TokioIo::new(server?);

        let _ = io::copy_bidirectional(&mut client, &mut server).await;
        Ok(())
    }
}
impl<P> ProxyOutBoundDefaultMethods for P where P: ProxyOutBound + ?Sized {}

pub type ProxyStack<'a> =
    Box<dyn ClonableIterator<Item = &'a dyn ProxyOutBound> + Send + Sync + 'a>;
pub trait ClonableIterator: Iterator + DynClone {}
impl<T: Iterator + DynClone> ClonableIterator for T {}
