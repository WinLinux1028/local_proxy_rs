use super::ProxyOutBound;
use crate::{outbound::ProxyOutBoundDefaultMethods, utils::ParsedUri, Connection, Error};

use base64::Engine;
use hyper::{Body, Request, Response, StatusCode, Uri};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use async_trait::async_trait;

pub struct HttpProxy {
    hostname: String,
    port: u16,
    auth: Option<String>,
}

impl HttpProxy {
    pub fn new(uri: Uri) -> Result<Self, Error> {
        let uri: ParsedUri = uri.try_into()?;

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
            hostname: uri.hostname().ok_or("")?.to_string(),
            port: uri.port.ok_or("")?,
            auth,
        })
    }
}

#[async_trait]
impl ProxyOutBound for HttpProxy {
    async fn connect(
        &self,
        mut proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        hostname: &str,
        port: u16,
    ) -> Result<Connection, Error> {
        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.hostname, self.port)
            .await?;
        let mut server = BufReader::new(server);

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
        if server.read_line(&mut response).await? == 0 {
            return Err("".into());
        }
        let mut response_code = response.split(' ');
        response_code.next();
        let response_code: u16 = response_code.next().ok_or("")?.parse()?;
        if !(200..=299).contains(&response_code) {
            return Err("".into());
        }

        while response != "\r\n" {
            response.clear();
            if server.read_line(&mut response).await? == 0 {
                return Err("".into());
            }
        }

        Ok(Box::new(server))
    }

    async fn http_proxy(
        &self,
        mut proxies: Box<dyn Iterator<Item = &Box<dyn ProxyOutBound>> + Send>,
        scheme: &str,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        if scheme != "http" {
            return self.http_proxy_(proxies, scheme, request).await;
        }

        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.hostname, self.port)
            .await?;
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
        response.headers_mut().remove("transfer-encoding");
        let server = hyper::upgrade::on(&mut response);
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            tokio::spawn(Self::proxy_upgrade(client, server));
        }

        Ok(response)
    }
}
