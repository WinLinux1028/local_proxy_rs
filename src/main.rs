mod connect;
mod outbound;
mod utils;

use crate::{outbound::ProxyOutBound, utils::UnSplit};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode, Uri,
};
use std::{
    io::Write,
    net::{SocketAddr, ToSocketAddrs},
};
use tokio::io::{AsyncBufRead, AsyncWrite};

use once_cell::sync::OnceCell;

static PROXY: OnceCell<ProxyState> = OnceCell::new();
type Error = Box<dyn std::error::Error + Send + Sync>;
type Connection =
    UnSplit<Box<dyn AsyncBufRead + Unpin + Sync + Send>, Box<dyn AsyncWrite + Unpin + Sync + Send>>;

#[tokio::main]
async fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut buf = String::new();

    write!(&mut stdout, "listen(like 127.0.0.1:8080)> ").unwrap();
    stdout.flush().unwrap();
    stdin.read_line(&mut buf).unwrap();
    let listen: SocketAddr = buf.trim().parse().unwrap();
    buf.clear();

    write!(
        &mut stdout,
        "http proxy(like http://user:password@example.com:8080)> "
    )
    .unwrap();
    stdout.flush().unwrap();
    stdin.read_line(&mut buf).unwrap();

    let proxy = buf.trim();

    if proxy.is_empty() {
        main_(listen, Box::new(outbound::Raw::new())).await;
    } else {
        let proxy: Uri = proxy.parse().unwrap();
        let proxy_protocol = proxy.scheme_str().unwrap();

        let proxy_host = proxy.host().unwrap();
        let proxy_port = proxy.port_u16().ok_or("").unwrap();
        let proxy_addr = format!("{}:{}", proxy_host, proxy_port)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        let proxy_auth_: Vec<&str> = proxy.authority().unwrap().as_str().split('@').collect();
        let mut proxy_auth = (None, None);
        if proxy_auth_.len() == 2 {
            let proxy_auth_: Vec<&str> = proxy_auth_[0].split(':').collect();
            if proxy_auth_.len() == 1 {
                proxy_auth = (Some(proxy_auth_[0]), None);
            } else if proxy_auth_.len() == 2 {
                proxy_auth = (Some(proxy_auth_[0]), Some(proxy_auth_[1]));
            }
        }

        if proxy_protocol == "http" {
            let proxy = outbound::HttpProxy::new(proxy_addr, proxy_auth.0, proxy_auth.1);
            main_(listen, Box::new(proxy)).await;
        }
    }
}

async fn main_(listen: SocketAddr, outbound: Box<dyn ProxyOutBound>) {
    PROXY.set(ProxyState { outbound }).unwrap();

    Server::try_bind(&listen)
        .unwrap()
        .http1_only(true)
        .tcp_nodelay(true)
        .serve(make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(handle))
        }))
        .await
        .unwrap();
}

async fn handle(request: Request<Body>) -> Result<Response<Body>, Error> {
    if request.method() == Method::CONNECT {
        connect::run(request).await
    } else if request.method() == Method::TRACE {
        Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())?)
    } else {
        let proxy = PROXY.get().ok_or("")?;
        proxy.outbound.http_proxy(request).await
    }
}

#[derive(Debug)]
struct ProxyState {
    outbound: Box<dyn ProxyOutBound>,
}
