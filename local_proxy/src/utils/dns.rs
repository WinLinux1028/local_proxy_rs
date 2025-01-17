use super::{Body, HostName};
use crate::{
    inbound::http::http_proxy::{self, RequestConfig},
    Error, PROXY,
};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Uri};
use std::{str::FromStr, time::Duration};

pub async fn doh_query(mut query: Vec<u8>) -> Result<Vec<u8>, Error> {
    let id = (*query.first().ok_or("")?, *query.get(1).ok_or("")?);
    *query.get_mut(0).ok_or("")? = 0xab;
    *query.get_mut(1).ok_or("")? = 0xcd;
    let proxy = PROXY.get().ok_or("")?;
    if let Some(s) = proxy.dns_cache.read().await.get(&query) {
        let mut result = s.clone();
        *result.get_mut(0).ok_or("")? = id.0;
        *result.get_mut(1).ok_or("")? = id.1;

        return Ok(result);
    }

    let proxy = PROXY.get().unwrap();
    let doh_config = proxy.config.doh.as_ref().ok_or("")?;
    let endpoint = Uri::from_str(&doh_config.endpoint)?;

    let request = Request::builder()
        .method(Method::POST)
        .uri(endpoint)
        .header("accept", "application/dns-message")
        .header("content-type", "application/dns-message")
        .body(Body::new(Full::new(Bytes::copy_from_slice(&query))))?;

    let mut req_conf = RequestConfig::new();
    req_conf.doh = false;
    req_conf.fake_host = match &doh_config.fake_host {
        Some(f) => HostName::from_str(f).ok(),
        _ => None,
    };
    if let Some(1) = &proxy.config.fragment {
        req_conf.fragment = Some(true)
    }

    let mut response = http_proxy::send_request(request, &req_conf).await?;
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

    *response_body.get_mut(0).ok_or("")? = id.0;
    *response_body.get_mut(1).ok_or("")? = id.1;

    Ok(response_body)
}
