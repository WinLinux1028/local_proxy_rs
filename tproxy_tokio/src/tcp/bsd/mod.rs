mod pf;

use crate::{RedirType, TcpListenerRedirExt, TcpStreamRedirExt};

use std::{
    io::{self, Error, ErrorKind},
    net::SocketAddr,
};
use tokio::net::{TcpListener, TcpStream};

use async_trait::async_trait;
use socket2::Protocol;

#[async_trait]
impl TcpListenerRedirExt for TcpListener {
    async fn bind_redir(ty: RedirType, addr: SocketAddr) -> Result<TcpListener, Error> {
        match ty {
            #[cfg(any(
                target_os = "openbsd",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "macos",
                target_os = "ios",
            ))]
            RedirType::PacketFilter => {}

            #[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios",))]
            RedirType::IpFirewall => {}

            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "not supported tcp transparent proxy type",
                ))
            }
        }

        // BSD platform doesn't have any special logic
        let listener = TcpListener::bind(addr).await?;

        Ok(listener)
    }
}

impl TcpStreamRedirExt for TcpStream {
    fn destination_addr(&self, ty: RedirType) -> io::Result<SocketAddr> {
        match ty {
            #[cfg(any(
                target_os = "openbsd",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "macos",
                target_os = "ios",
            ))]
            RedirType::Redirect => {
                let peer_addr = self.peer_addr()?;
                let bind_addr = self.local_addr()?;

                pf::PF.natlook(&bind_addr, &peer_addr, Protocol::TCP)
            }

            #[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios",))]
            RedirType::IpFirewall => {
                // For IPFW, uses getsockname() to retrieve destination address
                // FreeBSD: https://www.freebsd.org/doc/handbook/firewalls-ipfw.html
                self.local_addr()
            }
            _ => Err(Error::new(
                ErrorKind::InvalidInput,
                "not supported tcp transparent proxy type",
            )),
        }
    }
}
