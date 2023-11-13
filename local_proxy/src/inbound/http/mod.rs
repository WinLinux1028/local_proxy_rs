mod connect;
pub mod http_proxy;

use crate::{Error, ERROR_HTML, PROXY};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use std::time::Duration;

pub async fn start() -> Result<(), Error> {
    let listen = PROXY.get().unwrap().config.http_listen.ok_or("")?;
    Server::try_bind(&listen)
        .unwrap()
        .http1_only(true)
        .http1_header_read_timeout(Duration::from_secs(15))
        .tcp_nodelay(true)
        .serve(make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(handle))
        }))
        .await?;

    Ok(())
}

async fn handle(request: Request<Body>) -> Result<Response<Body>, Error> {
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
            .body(Body::from(ERROR_HTML))?);
    }

    response
}
