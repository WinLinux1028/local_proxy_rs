use crate::{
    utils::{self, SocketAddr},
    Error, PROXY,
};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
};
use tproxy_tokio::{RedirType, TcpListenerRedirExt, TcpStreamRedirExt};

static REDIR_TYPE: RedirType = RedirType::tcp_default();

pub async fn start() -> Result<(), Error> {
    let listen = PROXY.get().unwrap().config.tproxy_listen.ok_or("")?;
    let listener = TcpListener::bind_redir(REDIR_TYPE, listen).await?;

    loop {
        let client = match listener.accept().await {
            Ok((o, _)) => o,
            Err(_) => continue,
        };
        match client.set_nodelay(true) {
            Ok(_) => tokio::spawn(run(client)),
            Err(_) => continue,
        };
    }
}

async fn run<RW>(client: RW) -> Result<(), Error>
where
    RW: AsyncRead + AsyncWrite + TcpStreamRedirExt + Unpin + Send + 'static,
{
    let addr: SocketAddr = client.destination_addr(REDIR_TYPE)?.into();

    let mut proxies = PROXY.get().unwrap().proxy_stack.iter().rev();
    let server_conn = proxies
        .next()
        .ok_or("")?
        .connect(Box::new(proxies), &addr)
        .await?;

    utils::copy_bidirectional(client, server_conn).await;

    Ok(())
}
