use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub proxies: Option<Vec<ProxyConfig>>,
    pub doh_endpoint: Option<String>,
    pub listen: SocketAddr,
}

#[derive(Serialize, Deserialize)]
pub struct ProxyConfig {
    pub protocol: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub server: String,
}
