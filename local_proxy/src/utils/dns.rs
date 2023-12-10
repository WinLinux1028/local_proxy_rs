use super::Body;
use crate::{inbound::http::http_proxy, Error};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Uri};

pub async fn doh_query(endpoint: &Uri, query: Vec<u8>) -> Result<Vec<u8>, Error> {
    let request = Request::builder()
        .method(Method::POST)
        .uri(endpoint)
        .header("accept", "application/dns-message")
        .header("content-type", "application/dns-message")
        .header("content-length", query.len().to_string())
        .body(Body::new(Full::new(Bytes::from(query))))?;

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

    Ok(response_body)
}
