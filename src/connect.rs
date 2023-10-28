use crate::{
    utils::{self, ParsedUri},
    Error, PROXY,
};

use dns_parser::QueryType;
use hyper::{Body, Request, Response};

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    let server: ParsedUri = request.uri().clone().try_into()?;

    let server_hostname = server.hostname().ok_or("")?;
    let server_port = server.port.ok_or("")?;

    let proxy = PROXY.get().ok_or("")?;

    let server_conn;
    tokio::select! {
        Ok(conn) = async {
            let server_ip = utils::dns_resolve(QueryType::AAAA, server.hostname().ok_or("")?).await?;
            let mut proxies = proxy.proxy_stack.iter().rev();
            let conn = proxies
                .next()
                .ok_or("")?
                .connect(Box::new(proxies), &format!("[{}]", server_ip), server_port)
                .await?;
            Ok::<_, Error>(conn)
        } => server_conn = conn,
        Ok(conn) = async {
            let server_ip = utils::dns_resolve(QueryType::A, server.hostname().ok_or("")?).await?;
            let mut proxies = proxy.proxy_stack.iter().rev();
            let conn = proxies
                .next()
                .ok_or("")?
                .connect(Box::new(proxies), &server_ip.to_string(), server_port)
                .await?;
            Ok::<_, Error>(conn)
        } => server_conn = conn,
        else => {
            let mut proxies = proxy.proxy_stack.iter().rev();
            server_conn = proxies
                .next()
                .ok_or("")?
                .connect(Box::new(proxies), server_hostname, server_port)
                .await?;
        }
    }

    tokio::spawn(async {
        let client = hyper::upgrade::on(request).await?;
        utils::copy_bidirectional(client, server_conn).await;
        Ok::<_, Error>(())
    });

    Ok(Response::new(Body::empty()))
}
