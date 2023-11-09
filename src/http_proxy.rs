use crate::{
    utils::{HostName, ParsedUri, SocketAddr},
    Error, PROXY,
};

use base64::Engine;
use hyper::{header::HeaderValue, Body, Request, Response};

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    send_request(request, true).await
}

pub async fn send_request(
    mut request: Request<Body>,
    use_doh: bool,
) -> Result<Response<Body>, Error> {
    let mut uri: ParsedUri = request.uri().clone().try_into()?;

    if let Some(scheme) = uri.scheme() {
        match scheme {
            "http" => (),
            "https" => (),
            _ => return Err("".into()),
        }

        if request.headers().get("authorization").is_none() {
            if let Some(user) = uri.user() {
                let base64 = base64::engine::general_purpose::STANDARD;
                let mut auth_ = "Basic ".to_string();
                if let Some(password) = uri.password() {
                    auth_.push_str(&base64.encode(format!("{}:{}", user, password)));
                } else {
                    auth_.push_str(&base64.encode(format!("{}:", user)));
                }

                request
                    .headers_mut()
                    .insert("authorization", auth_.parse()?);
            }
        }
    } else {
        uri.scheme = Some("http".to_string());

        let host = request.headers().get("host").ok_or("")?.to_str()?;
        let (hostname, port) = SocketAddr::parse_host_header(host)?;
        uri.hostname = Some(hostname);
        uri.port = port;
    }

    let mut value_ = None;
    if let Some(value) = request.headers_mut().get_mut("te") {
        for i in value.to_str()?.split(',').map(|v| v.trim()) {
            match i.split(';').next() {
                Some("trailers") => {
                    value_ = Some(HeaderValue::from_static("trailers"));
                    break;
                }
                _ => continue,
            }
        }
    }
    match value_ {
        Some(s) => request.headers_mut().insert("te", s),
        None => request.headers_mut().remove("te"),
    };

    let proxy_header: Vec<String> = request
        .headers()
        .keys()
        .map(|i| i.as_str())
        .filter(|i| i.starts_with("proxy-"))
        .map(|i| i.to_string())
        .collect();
    for i in proxy_header {
        request.headers_mut().remove(i);
    }

    let scheme = uri.scheme().ok_or("")?.to_string();
    let hostname = uri.hostname().ok_or("")?;
    let mut host_header;
    if let HostName::V6(v6) = hostname {
        host_header = format!("[{}]", v6);
    } else {
        host_header = hostname.to_string();
    }
    if let Some(port) = uri.port {
        if (scheme == "http" && port == 80) || (scheme == "https" && port == 443) {
            uri.port = None;
        } else {
            host_header.push(':');
            host_header.push_str(&port.to_string());
        }
    }
    request.headers_mut().insert("host", host_header.parse()?);

    uri.scheme = None;
    uri.user = None;
    uri.password = None;
    uri.hostname = None;
    uri.port = None;

    *request.uri_mut() = uri.try_into()?;

    let proxy = PROXY.get().ok_or("")?;
    let mut proxies = proxy.proxy_stack.iter().rev();
    proxies
        .next()
        .ok_or("")?
        .http_proxy(Box::new(proxies), &scheme, use_doh, request)
        .await
}
