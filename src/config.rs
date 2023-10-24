use crate::Error;

use std::net::SocketAddr;

use hyper::Uri;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub proxy: Option<ProxyConfig>,
    pub listen: SocketAddr,
}

#[derive(Serialize, Deserialize)]
pub struct ProxyConfig {
    protocol: String,
    server: String,
}

impl ProxyConfig {
    pub fn to_uri(&self, user: Option<&str>, password: Option<&str>) -> Result<Uri, Error> {
        let mut uri = Uri::builder().scheme(self.protocol.as_str());

        let mut authority = String::new();
        if let Some(user) = user {
            authority.push_str(user);
            if let Some(password) = password {
                authority.push(':');
                authority.push_str(password);
            }
            authority.push('@');
        }
        authority.push_str(&self.server);
        uri = uri.authority(authority);

        uri = uri.path_and_query("");

        Ok(uri.build()?)
    }
}
