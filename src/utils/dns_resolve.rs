use crate::{http_proxy, Error, PROXY};

use bytes::Bytes;
use dns_message_parser::{
    question::{QClass, QType, Question},
    rr::RR,
};
use hyper::{body::HttpBody, Body, Method, Request, Uri};
use std::{net::IpAddr, str::FromStr};

pub async fn dns_resolve(q_type: QType, domain: &str) -> Result<IpAddr, Error> {
    if let Ok(addr) = IpAddr::from_str(domain) {
        return Ok(addr);
    }

    let query = Question {
        domain_name: domain.parse()?,
        q_class: QClass::IN,
        q_type,
    };
    let query: [Result<_, Error>; 1] = [Ok(query.encode()?)];
    let query = futures_util::stream::iter(query);

    let proxy = PROXY.get().ok_or("")?;
    let uri = Uri::from_str(proxy.config.doh_endpoint.as_ref().ok_or("")?)?;
    let request = Request::builder()
        .method(Method::POST)
        .uri(&uri)
        .header("accept", "application/dns-message")
        .header("content-type", "application/dns-message")
        .body(Body::wrap_stream(query))?;

    let mut response = http_proxy::run(request).await?;
    if !response.status().is_success() {
        return Err("".into());
    }

    let mut response_body = Vec::new();
    while let Some(chunk) = response.body_mut().data().await {
        response_body.extend_from_slice(chunk?.as_ref());
    }

    match RR::decode(Bytes::from(response_body))? {
        RR::A(addr) => Ok(IpAddr::V4(addr.ipv4_addr)),
        RR::AAAA(addr) => Ok(IpAddr::V6(addr.ipv6_addr)),
        _ => Err("".into()),
    }
}
