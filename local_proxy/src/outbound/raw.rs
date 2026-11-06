use super::ProxyOutBound;
use crate::{outbound::ProxyStack, utils::SocketAddr, Connection, Error};

use async_trait::async_trait;
use tokio::net::TcpStream;

/// 追加のプロキシを使わず, 宛先へ直接接続するoutbound
pub struct Raw();

impl Raw {
    pub fn new() -> Self {
        Self()
    }
}

#[async_trait]
impl ProxyOutBound for Raw {
    async fn connect(
        &self,
        mut proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        // 経由するプロキシが残っている場合, そのプロキシに接続処理を任せる
        if let Some(proxy) = proxies.next() {
            return proxy.connect(proxies, addr).await;
        }

        // 宛先へ直接接続する
        let server = TcpStream::connect(addr.to_string()).await?;
        server.set_nodelay(true)?; // 小さなパケットを遅延させるNagleアルゴリズムを無効にする

        Ok(Box::new(server))
    }
}
