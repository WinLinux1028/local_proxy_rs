#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod config;
mod connect;
mod http_proxy;
mod outbound;
mod utils;

use crate::{config::Config, outbound::ProxyOutBound};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use std::{io::Write, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};

use once_cell::sync::OnceCell;

static PROXY: OnceCell<ProxyState> = OnceCell::new();
type Error = Box<dyn std::error::Error + Sync + Send>;
type Connection = Box<dyn Stream + Unpin + Send>;

#[tokio::main]
async fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    let config: Config =
        serde_json::from_reader(std::fs::File::open("./config.json").unwrap()).unwrap();

    let mut proxy_stack: Vec<Box<dyn ProxyOutBound>> = vec![Box::new(outbound::Raw::new())];
    if let Some(proxies) = &config.proxies {
        for proxy in proxies {
            writeln!(
                &mut stdout,
                "Configuration of {}://{}",
                &proxy.protocol, &proxy.server
            )
            .unwrap();

            write!(&mut stdout, "proxy user> ").unwrap();
            stdout.flush().unwrap();
            let mut user = String::new();
            stdin.read_line(&mut user).unwrap();
            let user = user.trim();
            let user = if user.is_empty() { None } else { Some(user) };

            write!(&mut stdout, "proxy password> ").unwrap();
            stdout.flush().unwrap();
            let mut password = String::new();
            stdin.read_line(&mut password).unwrap();
            let password = password.trim();
            let password = if password.is_empty() {
                None
            } else {
                Some(password)
            };

            write!(&mut stdout, "\x1B[H\x1B[2J\x1B[3J").unwrap();
            stdout.flush().unwrap();

            let proxy = proxy.to_uri(user, password).unwrap();
            let proxy_protocol: Vec<&str> = proxy.scheme_str().unwrap().split('+').collect();

            for layer in &proxy_protocol[0..proxy_protocol.len() - 1] {
                if *layer == "tls" {
                    proxy_stack.push(Box::new(outbound::layer::TlsClient {}));
                } else {
                    panic!("This protocol can not use: {}", layer);
                }
            }

            let proxy_protocol_main = proxy_protocol[proxy_protocol.len() - 1];
            if proxy_protocol_main == "http" {
                proxy_stack.push(Box::new(outbound::HttpProxy::new(proxy).unwrap()));
            } else {
                panic!("This protocol can not use: {}", proxy_protocol_main);
            }
        }
    }

    if PROXY
        .set(ProxyState {
            config,
            proxy_stack,
        })
        .is_err()
    {
        panic!("Could not set to OnceCell");
    }

    let server = Server::try_bind(&PROXY.get().unwrap().config.listen)
        .unwrap()
        .http1_only(true)
        .http1_header_read_timeout(Duration::from_secs(15))
        .tcp_nodelay(true)
        .serve(make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(handle))
        }));
    println!("Server started");
    server.await.unwrap();
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
    config: Config,
    proxy_stack: Vec<Box<dyn ProxyOutBound>>,
}

trait Stream: AsyncRead + AsyncWrite {}
impl<RW> Stream for RW where RW: AsyncRead + AsyncWrite {}
