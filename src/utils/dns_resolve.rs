use crate::{http_proxy, utils::ParsedUri, Error, PROXY};

use base64::Engine;
use dns_parser::{QueryClass, QueryType, RData};
use hyper::{body::HttpBody, Body, Method, Request, Uri};
use std::{net::IpAddr, str::FromStr};

pub async fn dns_resolve(qtype: QueryType, domain: &str) -> Result<IpAddr, Error> {
    if let Ok(addr) = IpAddr::from_str(domain) {
        return Ok(addr);
    }

    let mut query = dns_parser::Builder::new_query(0xabcd, true);
    query.add_question(domain, false, qtype, QueryClass::IN);
    let query = query.build().map_err(|_| "")?;

    let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let query = base64.encode(query);

    let proxy = PROXY.get().ok_or("")?;
    let mut uri: ParsedUri =
        Uri::from_str(proxy.config.doh_endpoint.as_ref().ok_or("")?)?.try_into()?;
    if let Some(s) = uri.query.as_mut() {
        s.push_str(&format!("&dns={}", query));
    } else {
        uri.query = Some(format!("dns={}", query));
    }
    let uri: Uri = uri.try_into()?;

    let request = Request::builder()
        .method(Method::GET)
        .uri(&uri)
        .header("accept", "application/dns-message")
        .body(Body::empty())?;

    let mut response = http_proxy::run(request).await?;
    if !response.status().is_success() {
        return Err("".into());
    }

    let mut response_body = Vec::new();
    while let Some(chunk) = response.body_mut().data().await {
        response_body.extend_from_slice(chunk?.as_ref());
    }
    let response_body = dns_parser::Packet::parse(&response_body)?;

    for answer in response_body.answers {
        if answer.cls != dns_parser::Class::IN {
            continue;
        }
        match answer.data {
            RData::A(addr) => return Ok(IpAddr::V4(addr.0)),
            RData::AAAA(addr) => return Ok(IpAddr::V6(addr.0)),
            _ => continue,
        }
    }

    Err("".into())
}
