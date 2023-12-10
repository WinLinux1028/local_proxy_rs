mod connect;
pub mod http_proxy;

use crate::{utils::Body, Error, ERROR_HTML, PROXY};

use std::time::Duration;
use tokio::net::TcpListener;

use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;

pub async fn start() -> Result<(), Error> {
    let listen = PROXY.get().unwrap().config.http_listen.as_ref().ok_or("")?;
    if listen.is_empty() {
        return Ok(());
    }

    for i in listen {
        let listener = TcpListener::bind(i).await?;
        tokio::spawn(async move {
            loop {
                let client = match listener.accept().await {
                    Ok((o, _)) => TokioIo::new(o),
                    Err(_) => continue,
                };

                tokio::spawn(async {
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(client, service_fn(handle))
                        .await;
                });
            }
        });
    }

    loop {
        tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
    }
}

async fn handle(request: Request<Incoming>) -> Result<Response<Body>, Error> {
    let request = Body::convert_request(request);

    let mut response = if request.method() == Method::CONNECT {
        connect::run(request).await
    } else {
        http_proxy::run(request).await
    };

    if response.is_err() {
        response = Ok(Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .header("connection", "keep-alive")
            .header("content-type", "text/html; charset=utf-8")
            .header("content-length", ERROR_HTML.len().to_string())
            .body(Body::new(Full::new(Bytes::from(ERROR_HTML))))?);
    }

    response
}
