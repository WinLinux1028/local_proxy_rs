use super::{ProxyOutBound, ProxyOutBoundDefaultMethods};
use crate::{
    config::ProxyConfig,
    inbound::http::http_proxy::RequestConfig,
    outbound::ProxyStack,
    utils::{Body, SocketAddr},
    Connection, Error,
};

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use http_body_util::Empty;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use std::str::FromStr;

pub struct HttpProxy {
    addr: SocketAddr,
    auth: Option<String>,
}

impl HttpProxy {
    pub fn new(conf: &ProxyConfig) -> Result<Self, Error> {
        let mut auth = String::new();

        if let Some(user) = &conf.user {
            auth += user;
        }
        auth += ":";
        if let Some(password) = &conf.password {
            auth += password;
        }

        let auth = if auth.len() > 1 {
            let base64 = base64::engine::general_purpose::STANDARD;
            Some(base64.encode(&auth))
        } else {
            None
        };

        Ok(Self {
            addr: SocketAddr::from_str(&conf.server)?,
            auth,
        })
    }
}

#[async_trait]
impl ProxyOutBound for HttpProxy {
    async fn connect(
        &self,
        mut proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;

        let server = TokioIo::new(server);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(server).await?;
        tokio::spawn(conn.with_upgrades());

        let addr_str = addr.to_string();
        let mut request = Request::builder()
            .method(Method::CONNECT)
            .uri(&addr_str)
            .header("host", &addr_str)
            .header("connection", "keep-alive")
            .header("proxy-connection", "keep-alive");
        if let Some(auth) = &self.auth {
            request = request.header("proxy-authorization", format!("Basic {}", auth));
        }
        let request = request.body(Empty::<Bytes>::new())?;

        let response = sender.send_request(request).await?;
        if !response.status().is_success() {
            return Err("".into());
        }

        Ok(Box::new(TokioIo::new(hyper::upgrade::on(response).await?)))
    }

    async fn http_proxy(
        &self,
        mut proxies: ProxyStack<'_>,
        scheme: &str,
        req_conf: &RequestConfig,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        if scheme != "http" {
            return self.http_proxy_(proxies, scheme, req_conf, request).await;
        }

        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;
        let server = TokioIo::new(server);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(server).await?;
        tokio::spawn(conn.with_upgrades());

        let uri = Uri::builder()
            .scheme(scheme)
            .authority(request.headers().get("host").ok_or("")?.to_str()?)
            .path_and_query(request.uri().path_and_query().ok_or("")?.as_str())
            .build()?;

        *request.uri_mut() = uri;
        if let Some(auth) = &self.auth {
            request
                .headers_mut()
                .insert("proxy-authorization", format!("Basic {}", auth).parse()?);
        }

        let client = hyper::upgrade::on(&mut request);
        let mut response = sender.send_request(request).await?;
        let server = hyper::upgrade::on(&mut response);
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            tokio::spawn(Self::proxy_upgrade(client, server));
        }

        Ok(Body::convert_response(response))
    }
}
