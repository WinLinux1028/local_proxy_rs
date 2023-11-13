use std::{
    io::{self, Error, ErrorKind},
    mem,
    net::SocketAddr,
    os::fd::AsRawFd,
};
use tokio::net::{TcpListener, TcpSocket};

pub fn create_tproxy_listener(addr: SocketAddr) -> io::Result<TcpListener> {
    let socket = match addr {
        SocketAddr::V4(_) => TcpSocket::new_v4()?,
        SocketAddr::V6(_) => TcpSocket::new_v6()?,
    };

    set_ip_transparent(libc::IPPROTO_IPV6, &socket)?;
    set_ip_transparent(libc::IPPROTO_IP, &socket)?;

    socket.bind(addr)?;

    // listen backlogs = 1024 as mio's default
    let listener = socket.listen(1024)?;

    Ok(listener)
}

pub fn set_ip_transparent(level: libc::c_int, socket: &TcpSocket) -> io::Result<()> {
    let fd = socket.as_raw_fd();

    let opt = match level {
        libc::IPPROTO_IP => libc::IP_TRANSPARENT,
        libc::IPPROTO_IPV6 => libc::IPV6_TRANSPARENT,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "level can only be IPPROTO_IP and IPPROTO_IPV6",
            ))
        }
    };

    let enable: libc::c_int = 1;

    unsafe {
        let ret = libc::setsockopt(
            fd,
            level,
            opt,
            &enable as *const _ as *const _,
            mem::size_of_val(&enable) as libc::socklen_t,
        );

        if ret != 0 {
            return Err(Error::last_os_error());
        }
    }

    Ok(())
}
