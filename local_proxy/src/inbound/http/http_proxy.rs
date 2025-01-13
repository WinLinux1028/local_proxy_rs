use crate::{
    outbound::{layer::Fragment, ProxyOutBound},
    utils::{Body, HostName, ParsedUri, SocketAddr},
    Error, PROXY,
};

use base64::Engine;
use hyper::{header::HeaderValue, Request, Response};

pub async fn run(request: Request<Body>) -> Result<Response<Body>, Error> {
    send_request(request, &RequestConfig::new()).await
}

pub async fn send_request(
    mut request: Request<Body>,
    req_conf: &RequestConfig,
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

    let exists_trailers = request.headers().get("te").map(|v| {
        v.to_str().map(|v| {
            v.split(',')
                .map(|v| v.split(';').next().unwrap_or(v).trim())
                .any(|v| v == "trailers")
        })
    });
    match exists_trailers {
        Some(Ok(true)) => request
            .headers_mut()
            .insert("te", HeaderValue::from_static("trailers")),
        _ => request.headers_mut().remove("te"),
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
    let mut host_header = hostname.to_string_url_style();
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
    let mut proxies: Vec<&Box<dyn ProxyOutBound>> = proxy.proxy_stack.iter().collect();
    let fragment_layer: Box<dyn ProxyOutBound> = Box::new(Fragment::new());
    if let Some(fragment) = req_conf.fragment {
        if let Some(2..) | None = proxy.config.fragment {
            if !fragment {
                proxies.pop();
            }
        } else if fragment {
            proxies.push(&fragment_layer);
        }
    }
    let mut proxies = Box::new(proxies.into_iter().map(|p| &**p).rev());

    let response = proxies
        .next()
        .ok_or("")?
        .http_proxy(proxies, &scheme, req_conf, request)
        .await?;

    Ok(response)
}

pub struct RequestConfig {
    pub doh: bool,
    pub fake_host: Option<HostName>,
    pub fragment: Option<bool>,
}

impl RequestConfig {
    pub fn new() -> Self {
        RequestConfig {
            doh: true,
            fake_host: None,
            fragment: None,
        }
    }
}
