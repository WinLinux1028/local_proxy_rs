use super::ProxyOutBound;
use crate::{
    utils::{self, ParsedUri, UnSplit},
    Connection, Error,
};

use base64::Engine;
use hyper::{Body, Request, Response, StatusCode, Uri};
use std::net::{SocketAddr, ToSocketAddrs};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

use async_trait::async_trait;

pub struct HttpProxy {
    addr: SocketAddr,
    hostname: String,
    auth: Option<String>,
    tls: bool,
}

impl HttpProxy {
    pub fn new(uri: Uri) -> Result<Self, Error> {
        let uri: ParsedUri = uri.try_into()?;

        let addr = format!("{}:{}", uri.hostname().ok_or("")?, uri.port.ok_or("")?)
            .to_socket_addrs()?
            .next()
            .ok_or("")?;

        let mut auth = None;
        if let Some(user) = uri.user() {
            let base64 = base64::engine::general_purpose::STANDARD;
            if let Some(password) = uri.password() {
                auth = Some(base64.encode(format!("{}:{}", user, password)));
            } else {
                auth = Some(base64.encode(format!("{}:", user)))
            }
        }

        Ok(HttpProxy {
            addr,
            hostname: uri.hostname().ok_or("")?.to_string(),
            auth,
            tls: uri.scheme().ok_or("")?.starts_with("tls+"),
        })
    }

    async fn connect_to_proxy_server(&self) -> Result<Connection, Error> {
        let server = TcpStream::connect(&self.addr).await?;
        server.set_nodelay(true)?;
        if self.tls {
            let server = utils::tls_connect(server, &self.hostname).await?;
            let server = tokio::io::split(server);
            let server = (BufReader::new(server.0), server.1);
            Ok(unsafe { UnSplit::new(Box::new(server.0), Box::new(server.1)) })
        } else {
            let server = tokio::io::split(server);
            let server = (BufReader::new(server.0), server.1);
            Ok(unsafe { UnSplit::new(Box::new(server.0), Box::new(server.1)) })
        }
    }
}

#[async_trait]
impl ProxyOutBound for HttpProxy {
    async fn connect(&self, hostname: &str, port: u16) -> Result<Connection, Error> {
        let mut server = self.connect_to_proxy_server().await?;

        server
            .write_all(
                format!(
                    "CONNECT {0}:{1} HTTP/1.1\r\nHost: {0}:{1}\r\nProxy-Connection: Keep-Alive\r\n",
                    hostname, port
                )
                .as_bytes(),
            )
            .await?;
        if let Some(auth) = &self.auth {
            server
                .write_all(format!("Proxy-Authorization: Basic {}\r\n", auth).as_bytes())
                .await?;
        }
        server.write_all(b"\r\n").await?;
        server.flush().await?;

        let mut response = String::new();
        while !response.ends_with("\r\n\r\n") {
            if server.read_line(&mut response).await? == 0 {
                return Err("".into());
            }
        }
        let response = Response::new(response);
        if !response.status().is_success() {
            return Err("".into());
        }

        Ok(server)
    }

    async fn http_proxy(
        &self,
        scheme: &str,
        _: &str,
        _: u16,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        let server = self.connect_to_proxy_server().await?;
        let (mut sender, conn) = hyper::client::conn::handshake(server).await?;
        tokio::spawn(conn);

        let uri = Uri::builder()
            .scheme(scheme)
            .authority(request.headers().get("host").ok_or("")?.to_str()?)
            .path_and_query(request.uri().path_and_query().ok_or("")?.as_str())
            .build()?;

        *request.uri_mut() = uri;
        request
            .headers_mut()
            .insert("proxy-connection", "keep-alive".parse()?);
        if let Some(auth) = &self.auth {
            request
                .headers_mut()
                .insert("proxy-authorization", format!("basic {}", auth).parse()?);
        }

        let client = hyper::upgrade::on(&mut request);
        let mut response = sender.send_request(request).await?;
        response.headers_mut().remove("keep-alive");
        let server = hyper::upgrade::on(&mut response);
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            tokio::spawn(super::proxy_upgrade(client, server));
        }

        Ok(response)
    }
}
