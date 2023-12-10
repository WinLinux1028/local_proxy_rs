use crate::{utils::doh_query, Error, PROXY};

use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, sync::mpsc};

use hyper::Uri;

pub async fn start() -> Result<(), Error> {
    let listen = PROXY.get().unwrap().config.dns_listen.as_ref().ok_or("")?;
    if listen.is_empty() {
        return Ok(());
    }

    for i in listen {
        let socket = UdpSocket::bind(i).await?;

        tokio::spawn(async move {
            let (sender, mut receiver) = mpsc::channel(1024);
            let sender = Arc::new(sender);
            loop {
                let mut query = Vec::with_capacity(65527);
                tokio::select! {
                    result = socket.recv_buf_from(&mut query) => {
                        let (_, from) = match result {
                            Ok(o) => o,
                            Err(_) => continue,
                        };

                        let sender = Arc::clone(&sender);
                        tokio::spawn(async move {
                            let _ = run(query, from, &sender).await;
                        });
                    }
                    result = receiver.recv() => {
                        let (buf, to) = match result {
                            Some(s) => s,
                            None => continue,
                        };
                        let _ = socket.send_to(&buf, to).await;
                    }
                }
            }
        });
    }

    loop {
        tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
    }
}

async fn run(
    buf: Vec<u8>,
    from: SocketAddr,
    sender: &mpsc::Sender<(Vec<u8>, SocketAddr)>,
) -> Result<(), Error> {
    let proxy = PROXY.get().unwrap();
    let uri = Uri::from_str(proxy.config.doh_endpoint.as_ref().ok_or("")?)?;

    let result = doh_query(&uri, buf).await?;

    sender.send((result, from)).await?;

    Ok(())
}
