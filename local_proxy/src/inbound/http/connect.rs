use std::str::FromStr;

use crate::{
    utils::{self, SocketAddr},
    Error,
};

use hyper::{Body, Request, Response};

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    let server = SocketAddr::from_str(&request.uri().to_string())?;
    let server_conn = server.happy_eyeballs().await?;

    tokio::spawn(async {
        let client = hyper::upgrade::on(request).await?;
        utils::copy_bidirectional(client, server_conn).await;
        Ok::<_, Error>(())
    });

    Ok(Response::new(Body::empty()))
}
