#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod config;
mod connect;
mod http_proxy;
mod outbound;
mod utils;

use crate::{config::Config, outbound::ProxyOutBound, utils::UnSplit};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use std::{io::Write, time::Duration};
use tokio::io::{AsyncBufRead, AsyncWrite};

use once_cell::sync::OnceCell;

static PROXY: OnceCell<ProxyState> = OnceCell::new();
type Error = Box<dyn std::error::Error + Sync + Send>;
type Connection = UnSplit<Box<dyn AsyncBufRead + Unpin + Send>, Box<dyn AsyncWrite + Unpin + Send>>;

#[tokio::main]
async fn main() {
    let config: Config =
        serde_json::from_reader(std::fs::File::open("./config.json").unwrap()).unwrap();

    let outbound: Box<dyn ProxyOutBound>;
    if let Some(proxy) = &config.proxy {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        write!(&mut stdout, "user> ").unwrap();
        stdout.flush().unwrap();
        let mut user = String::new();
        stdin.read_line(&mut user).unwrap();
        let user = if user.is_empty() {
            None
        } else {
            Some(user.trim())
        };

        write!(&mut stdout, "password> ").unwrap();
        stdout.flush().unwrap();
        let mut password = String::new();
        stdin.read_line(&mut password).unwrap();
        let password = if password.is_empty() {
            None
        } else {
            Some(password.trim())
        };

        let proxy = proxy.to_uri(user, password).unwrap();
        let proxy_protocol = proxy.scheme_str().unwrap();

        if proxy_protocol == "http" || proxy_protocol == "tls+http" {
            outbound = Box::new(outbound::HttpProxy::new(proxy).unwrap());
        } else {
            panic!("This protocol can not use.");
        }
    } else {
        outbound = Box::new(outbound::Raw::new());
    }

    if PROXY.set(ProxyState { outbound }).is_err() {
        panic!("Could not set to OnceCell");
    }

    Server::try_bind(&config.listen)
        .unwrap()
        .http1_only(true)
        .http1_header_read_timeout(Duration::from_secs(15))
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
        http_proxy::run(request).await
    }
}

struct ProxyState {
    outbound: Box<dyn ProxyOutBound>,
}
