use std::time::Duration;

use super::Body;
use crate::{inbound::http::http_proxy, Error, PROXY};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Uri};

pub async fn doh_query(endpoint: &Uri, mut query: Vec<u8>) -> Result<Vec<u8>, Error> {
    *query.get_mut(0).ok_or("")? = 0xab;
    *query.get_mut(1).ok_or("")? = 0xcd;
    let proxy = PROXY.get().ok_or("")?;
    if let Some(s) = proxy.dns_cache.read().await.get(&query) {
        return Ok(s.clone());
    }

    let request = Request::builder()
        .method(Method::POST)
        .uri(endpoint)
        .header("accept", "application/dns-message")
        .header("content-type", "application/dns-message")
        .body(Body::new(Full::new(Bytes::copy_from_slice(&query))))?;

    let mut response = http_proxy::send_request(request, false).await?;
    if !response.status().is_success() {
        return Err("".into());
    }

    let mut response_body = Vec::new();
    while let Some(frame) = response.body_mut().frame().await {
        if let Some(data) = frame?.data_mut() {
            let mut chunk = data.chunk();
            while !chunk.is_empty() {
                response_body.extend_from_slice(chunk);

                data.advance(chunk.len());
                chunk = data.chunk();
            }
        }
    }

    proxy
        .dns_cache
        .write()
        .await
        .insert(query, response_body.clone(), Duration::from_secs(3600));

    Ok(response_body)
}
