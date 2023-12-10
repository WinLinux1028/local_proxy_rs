use crate::{RedirType, TcpListenerRedirExt, TcpStreamRedirExt};

use std::{
    io::{self, Error, ErrorKind},
    net::SocketAddr,
};
use tokio::net::{TcpListener, TcpStream};

use async_trait::async_trait;

#[async_trait]
impl TcpListenerRedirExt for TcpListener {
    async fn bind_redir(_ty: RedirType, _addr: SocketAddr) -> io::Result<TcpListener> {
        let err = Error::new(
            ErrorKind::InvalidInput,
            "not supported tcp transparent proxy on this platform",
        );
        Err(err)
    }
}

impl TcpStreamRedirExt for TcpStream {
    fn destination_addr(&self, _ty: RedirType) -> io::Result<SocketAddr> {
        unreachable!("not supported tcp transparent on this platform")
    }
}
