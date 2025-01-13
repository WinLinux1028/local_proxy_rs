use crate::{utils::SocketAddr, Error, PROXY};

use std::time::Duration;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    net::TcpListener,
};
use tproxy_tokio::{RedirType, TcpListenerRedirExt, TcpStreamRedirExt};

pub async fn start() -> Result<(), Error> {
    let config = PROXY
        .get()
        .unwrap()
        .config
        .tproxy_listen
        .as_ref()
        .ok_or("")?;
    if config.listen.is_empty() {
        return Ok(());
    }

    let redir_type = config
        .redir_type
        .as_deref()
        .map(|t| t.parse())
        .unwrap_or(Ok(RedirType::tcp_default()))?;

    for i in &config.listen {
        let listener = TcpListener::bind_redir(redir_type, *i).await?;
        tokio::spawn(async move {
            loop {
                let client = match listener.accept().await {
                    Ok((o, _)) => o,
                    Err(_) => continue,
                };
                match client.set_nodelay(true) {
                    Ok(_) => tokio::spawn(run(client, redir_type)),
                    Err(_) => continue,
                };
            }
        });
    }

    loop {
        tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
    }
}

async fn run<RW>(mut client: RW, redir_type: RedirType) -> Result<(), Error>
where
    RW: AsyncRead + AsyncWrite + TcpStreamRedirExt + Unpin + Send + 'static,
{
    let addr: SocketAddr = client.destination_addr(redir_type)?.into();

    let proxy = PROXY.get().ok_or("")?;
    let mut proxies = Box::new(proxy.proxy_stack.iter().map(|p| &**p).rev());
    let mut server_conn = proxies.next().ok_or("")?.connect(proxies, &addr).await?;

    let _ = io::copy_bidirectional(&mut client, &mut server_conn).await;

    Ok(())
}
