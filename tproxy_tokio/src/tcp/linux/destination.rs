use socket2::SockAddr;
use std::{
    io::{self, Error},
    net::SocketAddr,
    os::fd::AsRawFd,
};
use tokio::net::TcpStream;

pub fn get_original_destination_addr(s: &TcpStream) -> io::Result<SocketAddr> {
    let fd = s.as_raw_fd();

    unsafe {
        let (_, target_addr) = SockAddr::try_init(|target_addr, target_addr_len| {
            // No sufficient method to know whether the destination IPv4 or IPv6.

            let ret = libc::getsockopt(
                fd,
                libc::SOL_IP,
                libc::SO_ORIGINAL_DST,
                target_addr as *mut _,
                target_addr_len, // libc::socklen_t
            );

            if ret == 0 {
                return Ok(());
            }

            let ret = libc::getsockopt(
                fd,
                libc::SOL_IPV6,
                libc::IP6T_SO_ORIGINAL_DST,
                target_addr as *mut _,
                target_addr_len, // libc::socklen_t
            );

            if ret != 0 {
                let err = Error::last_os_error();
                return Err(err);
            }

            Ok(())
        })?;

        // Convert sockaddr_storage to SocketAddr
        Ok(target_addr.as_socket().expect("SocketAddr"))
    }
}
