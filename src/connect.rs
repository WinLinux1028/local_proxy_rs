use crate::{
    utils::{self, ParsedUri, UnSplit},
    Error, PROXY,
};

use hyper::{Body, Request, Response};
use tokio::io::{AsyncBufRead, AsyncWrite, BufReader};

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    let server: ParsedUri = request.uri().clone().try_into()?;

    let server_hostname = server.hostname().ok_or("")?;
    let server_port = server.port.ok_or("")?;

    let proxy = PROXY.get().ok_or("")?;

    let mut server_conn = None;
    //todo

    if server_conn.is_none() {
        server_conn = Some(
            proxy
                .outbound
                .connect(server_hostname.to_string(), server_port)
                .await?,
        );
    }

    tokio::spawn(tunnel(request, server_conn.ok_or("")?));
    Ok(Response::new(Body::empty()))
}

async fn tunnel<R, W>(request: Request<Body>, server: UnSplit<R, W>) -> Result<(), Error>
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let client = hyper::upgrade::on(request).await?;
    let client = tokio::io::split(client);
    let client = unsafe { UnSplit::new(BufReader::new(client.0), client.1) };

    utils::copy_bidirectional(client, server).await;
    Ok(())
}
