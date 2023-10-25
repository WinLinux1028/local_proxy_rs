use crate::{
    utils::{self, UnSplit},
    Error, PROXY,
};

use hyper::{Body, Request, Response};
use tokio::io::{AsyncBufRead, AsyncWrite, BufReader};

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    let server = request.uri().to_string();
    let server: Vec<&str> = server.split(':').collect();
    if server.len() != 2 {
        return Err("".into());
    }
    let server_hostname = server[0];
    let server_port: u16 = server[1].parse()?;

    let server_ips = utils::dns_resolve(server_hostname).await;

    let proxy = PROXY.get().ok_or("")?;
    //todo

    let server_conn = proxy
        .outbound
        .connect(server_hostname.to_string(), server_port)
        .await?;
    tokio::spawn(tunnel(request, server_conn));
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
