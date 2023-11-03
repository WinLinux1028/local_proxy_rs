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
use std::{
    hash::Hash,
    io::Write,
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::RwLock,
};
use ttl_cache::TtlCache;

use once_cell::sync::OnceCell;

static PROXY: OnceCell<ProxyState> = OnceCell::new();
type Error = Box<dyn std::error::Error + Sync + Send>;
type Connection = Box<dyn Stream + Unpin + Send>;

#[tokio::main]
async fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    let mut config: Config =
        serde_json::from_reader(std::fs::File::open("./config.json").unwrap()).unwrap();

    let mut proxy_stack: Vec<Box<dyn ProxyOutBound>> = vec![Box::new(outbound::Raw::new())];
    if let Some(proxies) = &mut config.proxies {
        for proxy in proxies {
            writeln!(
                &mut stdout,
                "Configuration of {}://{}",
                &proxy.protocol, &proxy.server
            )
            .unwrap();

            if proxy.user.is_none() {
                write!(&mut stdout, "proxy user> ").unwrap();
                stdout.flush().unwrap();
                let mut user = String::new();
                stdin.read_line(&mut user).unwrap();
                proxy.user = Some(user);
            }
            let user = proxy.user.as_mut().unwrap();
            *user = user.trim().to_string();
            if user.is_empty() {
                proxy.user = None;
            }

            if proxy.password.is_none() {
                write!(&mut stdout, "proxy password> ").unwrap();
                stdout.flush().unwrap();
                let mut password = String::new();
                stdin.read_line(&mut password).unwrap();
                proxy.password = Some(password.to_string());
            }
            let password = proxy.password.as_mut().unwrap();
            *password = password.trim().to_string();
            if password.is_empty() {
                proxy.password = None;
            }

            write!(&mut stdout, "\x1B[H\x1B[2J\x1B[3J").unwrap();
            stdout.flush().unwrap();

            let proxy_protocol: Vec<&str> = proxy.protocol.split('+').collect();
            for layer in &proxy_protocol[0..proxy_protocol.len() - 1] {
                match *layer {
                    "tls" => proxy_stack.push(Box::new(outbound::layer::TlsClient {})),
                    _ => panic!("This protocol can not use: {}", layer),
                }
            }

            let proxy_protocol_main = proxy_protocol[proxy_protocol.len() - 1];
            match proxy_protocol_main {
                "http" => proxy_stack.push(Box::new(outbound::HttpProxy::new(proxy).unwrap())),
                "socks4" => proxy_stack.push(Box::new(outbound::Socks4Proxy::new(proxy).unwrap())),
                "socks5" => proxy_stack.push(Box::new(outbound::Socks5Proxy::new(proxy).unwrap())),
                _ => panic!("This protocol can not use: {}", proxy_protocol_main),
            }
        }
    }

    let dns_cache = if config.doh_endpoint.is_some() {
        TtlCache::new(65535)
    } else {
        TtlCache::new(0)
    };

    if PROXY
        .set(ProxyState {
            config,
            dns_cache: RwLock::new(dns_cache),
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

#[allow(clippy::type_complexity)]
struct ProxyState {
    config: Config,
    dns_cache: RwLock<TtlCache<String, (DnsCacheState<Ipv4Addr>, DnsCacheState<Ipv6Addr>)>>,
    proxy_stack: Vec<Box<dyn ProxyOutBound>>,
}

enum DnsCacheState<T: Hash> {
    Some(T),
    Fail,
    None,
}

pub trait Stream: AsyncRead + AsyncWrite {}
impl<RW> Stream for RW where RW: AsyncRead + AsyncWrite {}
