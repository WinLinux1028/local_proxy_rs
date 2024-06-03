use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub proxies: Option<Vec<ProxyConfig>>,
    pub doh: Option<DoHConfig>,
    pub http_listen: Option<Vec<SocketAddr>>,
    pub tproxy_listen: Option<TProxy>,
    pub dns_listen: Option<Vec<SocketAddr>>,
}

#[derive(Serialize, Deserialize)]
pub struct ProxyConfig {
    pub protocol: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub server: String,
}

#[derive(Serialize, Deserialize)]
pub struct DoHConfig {
    pub endpoint: String,
    pub fake_host: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TProxy {
    pub listen: Vec<SocketAddr>,
    pub redir_type: Option<String>,
}
