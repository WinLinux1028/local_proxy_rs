pub mod http_proxy;

mod connect;

use crate::{utils::Body, Error, ERROR_HTML, PROXY};

use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    header::HeaderValue,
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use std::time::Duration;
use tokio::net::TcpListener;

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
                    Ok((o, _)) => o,
                    Err(_) => continue,
                };
                let client = match client.set_nodelay(true) {
                    Ok(_) => TokioIo::new(client),
                    Err(_) => continue,
                };

                tokio::spawn(async {
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(client, service_fn(handle))
                        .with_upgrades()
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

    let mut response;
    if request.method() == Method::CONNECT {
        response = connect::run(request).await;
    } else {
        response = http_proxy::run(request).await;
        if let Ok(response) = &mut response {
            response
                .headers_mut()
                .insert("connection", HeaderValue::from_static("keep-alive"));
            response.headers_mut().remove("keep-alive");
        }
    };

    if response.is_err() {
        response = Ok(Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .header("connection", "keep-alive")
            .header("content-type", "text/html; charset=utf-8")
            .body(Body::new(Full::new(Bytes::from(ERROR_HTML))))?);
    }

    response
}
