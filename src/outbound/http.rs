use super::ProxyOutBound;
use crate::{
    utils::{ParsedUri, UnSplit},
    Error,
};

use base64::Engine;
use hyper::{Response, Uri};
use std::net::{SocketAddr, ToSocketAddrs};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpStream,
};

use async_trait::async_trait;

#[derive(Debug)]
pub struct HttpProxy {
    addr: SocketAddr,
    auth: Option<String>,
}

impl HttpProxy {
    pub fn new(uri: Uri) -> Result<Self, Error> {
        let uri: ParsedUri = uri.try_into()?;

        let addr = format!("{}:{}", uri.host().ok_or("")?, uri.port.ok_or("")?)
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

        Ok(HttpProxy { addr, auth })
    }
}

#[async_trait]
impl ProxyOutBound for HttpProxy {
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
    > {
        let server = tokio::io::split(TcpStream::connect(&self.addr).await?);
        let mut server = (BufReader::new(server.0), server.1);

        server
            .1
            .write_all(
                format!(
                    "CONNECT {0}:{1} HTTP/1.1\r\nHost: {0}:{1}\r\nProxy-Connection: Keep-Alive\r\n",
                    addr, port
                )
                .as_bytes(),
            )
            .await?;
        if let Some(s) = &self.auth {
            server
                .1
                .write_all(format!("Proxy-Authorization: Basic {}\r\n", s).as_bytes())
                .await?;
        }
        server.1.write_all(b"\r\n").await?;
        server.1.flush().await?;

        let mut response = String::new();
        while !response.ends_with("\r\n\r\n") {
            if server.0.read_line(&mut response).await? == 0 {
                return Err("".into());
            }
        }
        let response = Response::new(response);
        if !response.status().is_success() {
            return Err("".into());
        }

        Ok(unsafe { UnSplit::new(Box::new(server.0), Box::new(server.1)) })
    }
}
