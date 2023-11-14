use crate::{inbound::http::http_proxy, Error, PROXY};

use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, sync::mpsc};

use hyper::{body::HttpBody, Body, Method, Request, Uri};

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

    let request = Request::builder()
        .method(Method::POST)
        .uri(&uri)
        .header("accept", "application/dns-message")
        .header("content-type", "application/dns-message")
        .header("content-length", buf.len().to_string())
        .body(Body::from(buf))?;

    let mut response = http_proxy::send_request(request, false).await?;
    if !response.status().is_success() {
        return Err("".into());
    }

    let mut response_body = Vec::new();
    while let Some(chunk) = response.body_mut().data().await {
        response_body.extend_from_slice(chunk?.as_ref());
    }

    sender.send((response_body, from)).await?;

    Ok(())
}
