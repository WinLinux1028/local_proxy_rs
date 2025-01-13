use crate::{
    outbound::ProxyOutBoundDefaultMethods,
    utils::{Body, SocketAddr},
    Error, PROXY,
};

use bytes::Bytes;
use http_body_util::Empty;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::str::FromStr;
use tokio::io;

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    let server = SocketAddr::from_str(&request.uri().to_string())?;

    let proxy = PROXY.get().ok_or("")?;
    let mut proxies = Box::new(proxy.proxy_stack.iter().map(|p| &**p).rev());
    let mut server_conn = proxies
        .next()
        .ok_or("")?
        .happy_eyeballs(proxies, &server)
        .await?;

    tokio::spawn(async move {
        let mut client = TokioIo::new(hyper::upgrade::on(request).await?);
        let _ = io::copy_bidirectional(&mut client, &mut server_conn).await;
        Ok::<_, Error>(())
    });

    Ok(Response::new(Body::new(Empty::<Bytes>::new())))
}
