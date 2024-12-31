mod bind;
mod destination;

use crate::{RedirType, TcpListenerRedirExt, TcpStreamRedirExt};

use std::{
    io::{self, Error, ErrorKind},
    net::SocketAddr,
};
use tokio::net::{TcpListener, TcpStream};

use async_trait::async_trait;

#[async_trait]
impl TcpListenerRedirExt for TcpListener {
    async fn bind_redir(ty: RedirType, addr: SocketAddr) -> io::Result<TcpListener> {
        match ty {
            RedirType::Redirect => {
                // REDIRECT rule doesn't need to set IP_TRANSPARENT
                let listener = TcpListener::bind(addr).await?;
                Ok(listener)
            }
            RedirType::TProxy => {
                // TPROXY rule requires IP_TRANSPARENT
                let listener = bind::create_tproxy_listener(addr)?;
                Ok(listener)
            }
            _ => Err(Error::new(
                ErrorKind::InvalidInput,
                "not supported tcp transparent proxy type",
            )),
        }
    }
}

impl TcpStreamRedirExt for TcpStream {
    fn destination_addr(&self, ty: RedirType) -> io::Result<SocketAddr> {
        match ty {
            RedirType::Redirect => destination::get_original_destination_addr(self),
            RedirType::TProxy => {
                // For TPROXY, uses getsockname() to retrieve original destination address
                self.local_addr()
            }
            _ => unreachable!("not supported tcp transparent proxy type"),
        }
    }
}
