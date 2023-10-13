use crate::{utils, Connection, Error, PROXY};

use hyper::{Body, Request, Response};
use tokio::io::BufReader;

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    let proxy = PROXY.get().ok_or("")?;

    let server = request.uri().to_string();
    let server: Vec<&str> = server.split(':').collect();
    if server.len() != 2 {
        return Err("".into());
    }
    let server_host = server[0];
    let server_port: u16 = server[1].parse()?;

    let server_conn = proxy.outbound.connect(server_host, server_port).await?;

    tokio::spawn(async move {
        let _ = tunnel(request, server_conn).await;
    });

    Ok(Response::new(Body::empty()))
}

async fn tunnel(request: Request<Body>, server: Connection) -> Result<(), Error> {
    let server = server.split();
    let client = hyper::upgrade::on(request).await?;
    let client = tokio::io::split(client);

    let client_to_server = tokio::spawn(async {
        let _ = utils::copy(BufReader::new(client.0), server.1).await;
    });
    let server_to_client = tokio::spawn(async {
        let _ = utils::copy(server.0, client.1).await;
    });

    tokio::select! {
        _ = client_to_server => {}
        _ = server_to_client => {}
    }

    Ok(())
}
