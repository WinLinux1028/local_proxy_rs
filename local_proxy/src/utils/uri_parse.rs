use crate::{
    utils::{HostName, SocketAddr},
    Error,
};

use hyper::Uri;
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};

#[derive(Clone)]
pub struct ParsedUri {
    pub scheme: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub hostname: Option<HostName>,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
}

#[allow(dead_code)]
impl ParsedUri {
    pub fn scheme(&self) -> Option<&str> {
        self.scheme.as_deref()
    }
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }
    pub fn hostname(&self) -> Option<&HostName> {
        self.hostname.as_ref()
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }
}

impl TryFrom<Uri> for ParsedUri {
    type Error = Error;

    fn try_from(value: Uri) -> Result<Self, Self::Error> {
        let mut scheme = None;
        let mut user = None;
        let mut password = None;
        let mut hostname = None;
        let mut port = None;
        let mut path = value.path();
        let mut query = value.query();

        if let Some(scheme_) = value.scheme_str() {
            scheme = Some(scheme_);
        }

        if let Some(authority) = value.authority() {
            let host;
            if let Some((auth, host_)) = authority.as_str().rsplit_once('@') {
                host = host_;
                if let Some((user_, password_)) = auth.split_once(':') {
                    user = Some(percent_decode_str(user_).decode_utf8()?.to_string());
                    password = Some(percent_decode_str(password_).decode_utf8()?.to_string());
                } else {
                    user = Some(percent_decode_str(auth).decode_utf8()?.to_string());
                }
            } else {
                host = authority.as_str();
            }

            let (hostname_, port_) = SocketAddr::parse_host_header(host)?;
            hostname = Some(hostname_);
            port = port_;
        }

        if scheme.is_some() && hostname.is_none() {
            return Err("".into());
        }
        if scheme.is_none() {
            if user.is_some() {
                return Err("".into());
            }
            if hostname.is_some()
                && (port.is_none() || (!path.is_empty()) || value.query().is_some())
            {
                return Err("".into());
            }
            if hostname.is_none() && (path.is_empty()) {
                return Err("".into());
            }
        }

        if path.is_empty() {
            path = "/";
        }

        if let Some(query_) = query {
            if query_.is_empty() {
                query = None;
            }
        }

        Ok(ParsedUri {
            scheme: scheme.map(|s| s.to_string()),
            user: user.map(|u| u.to_string()),
            password: password.map(|p| p.to_string()),
            hostname,
            port,
            path: path.to_string(),
            query: query.map(|q| q.to_string()),
        })
    }
}

impl TryInto<Uri> for ParsedUri {
    type Error = Error;

    fn try_into(self) -> Result<Uri, Self::Error> {
        let mut uri = Uri::builder();
        if let Some(scheme) = self.scheme {
            uri = uri.scheme(scheme.as_str());
        }

        let mut authority = String::new();
        if let Some(user) = self.user {
            authority = utf8_percent_encode(&user, NON_ALPHANUMERIC).to_string();
            if let Some(password) = self.password {
                authority.push(':');
                authority.push_str(&utf8_percent_encode(&password, NON_ALPHANUMERIC).to_string());
            }
            authority.push('@');
        }
        if let Some(hostname) = self.hostname {
            authority.push_str(&hostname.to_string_url_style());

            if let Some(port) = self.port {
                authority.push(':');
                authority.push_str(&port.to_string());
            }

            uri = uri.authority(authority);
        }

        let mut path_and_query = self.path;
        if let Some(query) = self.query {
            path_and_query.push('?');
            path_and_query.push_str(&query);
        }
        uri = uri.path_and_query(path_and_query);

        Ok(uri.build()?)
    }
}
