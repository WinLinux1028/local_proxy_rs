use hyper::Uri;

use crate::Error;

#[derive(Clone)]
pub struct ParsedUri {
    pub scheme: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub host: Option<String>,
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
    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
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
        let mut host = None;
        let mut port = None;

        if let Some(authority) = value.authority() {
            if let Some(scheme_) = value.scheme_str() {
                scheme = Some(scheme_.to_string());
                host = Some(authority.host().to_string());
                port = authority.port_u16();

                let auth: Vec<&str> = authority.as_str().split('@').collect();
                if auth.len() == 2 {
                    let mut auth = auth[0].split(':');
                    user = auth.next().map(|u| u.to_string());
                    password = auth.next().map(|p| p.to_string());

                    if auth.next().is_some() {
                        return Err("".into());
                    }
                } else if auth.len() != 1 {
                    return Err("".into());
                }
            }
        }

        Ok(ParsedUri {
            scheme,
            user,
            password,
            host,
            port,
            path: value.path().to_string(),
            query: value.query().map(|v| v.to_string()),
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
            authority = user;
            if let Some(password) = self.password {
                authority.push(':');
                authority.push_str(&password);
            }
            authority.push('@');
        }
        if let Some(host) = self.host {
            authority.push_str(&host);
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
