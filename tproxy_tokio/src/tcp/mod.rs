cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        mod linux;
    } else if #[cfg(any(target_os = "macos",
                        target_os = "ios",
                        target_os = "freebsd",
                        target_os = "netbsd",
                        target_os = "openbsd"))] {
        mod bsd;
    } else {
        mod other;
    }
}

use crate::RedirType;

use std::{
    io::{self},
    net::SocketAddr,
};
use tokio::net::TcpListener;

use async_trait::async_trait;
use cfg_if::cfg_if;

/// Extension function for `TcpListener` for setting extra options before `bind()`
#[async_trait]
pub trait TcpListenerRedirExt {
    // Create a TcpListener for transparent proxy
    //
    // Implementation is platform dependent
    async fn bind_redir(ty: RedirType, addr: SocketAddr) -> io::Result<TcpListener>;
}

/// Extension function for `TcpStream` for reading original destination address
pub trait TcpStreamRedirExt {
    // Read destination address for TcpStream
    //
    // Implementation is platform dependent
    fn destination_addr(&self, ty: RedirType) -> io::Result<SocketAddr>;
}
