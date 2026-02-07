use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Serialize, Deserialize)]
pub struct AppConfig {
    pub proxies: Option<Vec<ProxyConfig>>,
    pub doh: Option<DoHConfig>,
    pub fragment: Option<u8>,
    pub http_listen: Option<Vec<SocketAddr>>,
    pub tproxy_listen: Option<TProxyConfig>,
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
pub struct TProxyConfig {
    pub listen: Vec<SocketAddr>,
    pub redir_type: Option<String>,
}
