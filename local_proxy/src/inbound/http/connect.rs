use crate::{
    utils::{self, Body, SocketAddr},
    Error,
};

use std::str::FromStr;

use bytes::Bytes;
use http_body_util::Empty;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    let server = SocketAddr::from_str(&request.uri().to_string())?;
    let server_conn = server.happy_eyeballs().await?;

    tokio::spawn(async {
        let client = TokioIo::new(hyper::upgrade::on(request).await?);
        utils::copy_bidirectional(client, server_conn).await;
        Ok::<_, Error>(())
    });

    Ok(Response::new(Body::new(Empty::<Bytes>::new())))
}
