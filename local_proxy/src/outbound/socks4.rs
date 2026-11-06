use super::ProxyOutBound;
use crate::{
    config::ProxyConfig,
    outbound::ProxyStack,
    utils::{HostName, SocketAddr},
    Connection, Error,
};

use async_trait::async_trait;
use std::{net::Ipv4Addr, str::FromStr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Socks4Proxy {
    addr: SocketAddr,
    auth: Option<String>,
}

impl Socks4Proxy {
    pub fn new(conf: &ProxyConfig) -> Result<Self, Error> {
        // 認証情報を準備する
        let mut auth = String::new();

        if let Some(user) = &conf.user {
            auth += user;
        }
        if let Some(password) = &conf.password {
            auth += ":";
            auth += password;
        }

        // SOCKS4の認証情報はnull文字が終端のため, 値にNULが含まれている場合はエラーにする
        if auth.contains('\0') {
            return Err("".into());
        }

        // 認証情報が空文字列の場合は認証情報なしとする
        let auth = if !auth.is_empty() { Some(auth) } else { None };

        Ok(Self {
            addr: SocketAddr::from_str(&conf.server)?,
            auth,
        })
    }
}

#[async_trait]
impl ProxyOutBound for Socks4Proxy {
    /// SOCKS4 CONNECT要求を使い, 指定した宛先へのTCPトンネルを作る
    async fn connect(
        &self,
        mut proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        let ip;
        let mut hostname = None;
        match &addr.hostname {
            HostName::V4(v4) => {
                // 255.255.255.xや0.0.0.x のようなアドレスはエラーにする
                let v4_integer = u32::from_be_bytes(v4.octets());
                if v4_integer & 0xFFFFFF00 == 0 && v4_integer & 0xFF != 0 {
                    return Err("".into());
                }
                // 接続先アドレスをそのままセット
                ip = *v4
            }
            HostName::V6(v6) => {
                // ホスト名を入れるところにIPv6アドレスを入れる(SOCKS4a拡張の勝手拡張)
                ip = Ipv4Addr::new(0, 0, 0, 1);
                hostname = Some(v6.to_string());
            }
            HostName::Domain(domain) => {
                // ホスト名を指定する場合はIPアドレスは0.0.0.1を指定する(SOCKS4a拡張)
                ip = Ipv4Addr::new(0, 0, 0, 1);
                if domain.contains('\0') {
                    // ホスト名はnull文字で終端なので, null文字が含まれている場合はエラーにする
                    return Err("".into());
                }
                hostname = Some(domain.clone());
            }
        }

        // プロキシサーバーとの接続を確立する
        let mut server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;

        // リクエストを送信する
        server.write_all(&[4, 1]).await?; // Version Number = 4, Command Code = 1(CONNECT)
        server.write_all(&addr.port.to_be_bytes()).await?; // Destination Port
        server.write_all(&ip.octets()).await?; // Destination IP Address

        // User ID
        if let Some(auth) = &self.auth {
            server.write_all(auth.as_bytes()).await?
        }
        server.write_all(b"\0").await?; // Null Terminator

        // Destination Domain(SOCKS4a)
        if let Some(hostname) = &hostname {
            server.write_all(hostname.as_bytes()).await?;
            server.write_all(b"\0").await?;
        }
        server.flush().await?;

        // プロキシサーバーからの応答を受信する

        // Reply Version(0でなければエラー)
        if server.read_u8().await? != 0 {
            return Err("".into());
        }
        // Command Code(90でなければエラー)
        if server.read_u8().await? != 90 {
            return Err("".into());
        }
        // 残りの情報は使わないので読み捨てる
        let mut buf = [0; 6]; // Destination Port(2バイト) + Destination IP Address(4バイト)
        server.read_exact(&mut buf).await?;

        Ok(Box::new(server))
    }
}
