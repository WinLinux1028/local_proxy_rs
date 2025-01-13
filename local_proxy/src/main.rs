#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod config;
mod inbound;
mod outbound;
mod utils;

use crate::{config::Config, outbound::ProxyOutBound};

use once_cell::sync::OnceCell;
use std::io::{Read, Write};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::RwLock,
};
use ttl_cache::TtlCache;

static ERROR_HTML: &[u8] = include_bytes!("../static/error.html");
static PROXY: OnceCell<ProxyState> = OnceCell::new();
type Error = Box<dyn std::error::Error + Sync + Send>;
type Connection = Box<dyn Stream + Unpin + Send>;

#[tokio::main]
async fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    let mut config = String::new();
    std::fs::File::open("./config.json5")
        .unwrap()
        .read_to_string(&mut config)
        .unwrap();
    let mut config: Config = json5::from_str(&config).unwrap();

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
                let user = user.trim_end_matches(['\r', '\n']);
                proxy.user = Some(user.to_string());

                if proxy.password.is_none() {
                    write!(&mut stdout, "proxy password> ").unwrap();
                    stdout.flush().unwrap();
                    let mut password = String::new();
                    stdin.read_line(&mut password).unwrap();
                    let password = password.trim_end_matches(['\r', '\n']);
                    if !password.is_empty() {
                        proxy.password = Some(password.to_string());
                    }
                }
            }

            let user = proxy.user.as_mut().unwrap();
            if user.is_empty() {
                proxy.user = None;
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

    if let Some(2..) | None = config.fragment {
        proxy_stack.push(Box::new(outbound::layer::Fragment::new()));
    }

    let dns_cache = if config.doh.is_some() {
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

    let _ = tokio::join!(
        inbound::http::start(),
        inbound::tproxy::start(),
        inbound::dns::start(),
        async {
            println!("Server started");
        }
    );
}

#[allow(clippy::type_complexity)]
struct ProxyState {
    config: Config,
    dns_cache: RwLock<TtlCache<Vec<u8>, Vec<u8>>>,
    proxy_stack: Vec<Box<dyn ProxyOutBound>>,
}

pub trait Stream: AsyncRead + AsyncWrite {}
impl<RW> Stream for RW where RW: AsyncRead + AsyncWrite {}
