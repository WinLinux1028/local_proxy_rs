use crate::{utils::ParsedUri, Error, PROXY};

use base64::Engine;
use hyper::{Body, Request, Response};

pub async fn run(mut request: Request<Body>) -> Result<Response<Body>, Error> {
    let mut uri: ParsedUri = request.uri().clone().try_into()?;
    let mut new_uri = uri.clone();

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

        let mut host = request
            .headers()
            .get("host")
            .ok_or("")?
            .to_str()?
            .split(':');
        new_uri.host = Some(host.next().ok_or("")?.to_string());
        if let Some(port) = host.next() {
            new_uri.port = Some(port.parse()?);
        }
    }

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
    let host = uri.host().ok_or("")?.to_string();
    let port;
    let mut host_header = host.clone();
    if let Some(port_) = uri.port {
        port = port_;
        if (scheme == "http" && port == 80) || (scheme == "https" && port == 443) {
            uri.port = None;
        } else {
            host_header.push(':');
            host_header.push_str(&port.to_string());
        }
    } else {
        match scheme.as_str() {
            "http" => port = 80,
            "https" => port = 443,
            _ => return Err("".into()),
        }
    }
    request.headers_mut().insert("host", host_header.parse()?);

    uri.scheme = None;
    uri.user = None;
    uri.password = None;
    uri.host = None;
    uri.port = None;

    *request.uri_mut() = uri.try_into()?;

    let proxy = PROXY.get().ok_or("")?;
    proxy
        .outbound
        .http_proxy(&scheme, &host, port, request)
        .await
}
