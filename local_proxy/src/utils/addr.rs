use crate::{utils::doh_query, Error};

use dns_parser::{QueryClass, QueryType, RData};
use std::{
    fmt::{Display, Write},
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

/// IPアドレスに加えてドメインも入れられる, 接続先を表す構造体
#[derive(Clone)]
pub struct SocketAddr {
    pub hostname: HostName,
    pub port: u16,
}

impl SocketAddr {
    /// ホスト名とポートからそのまま構築する
    pub fn new(hostname: HostName, port: u16) -> Self {
        Self { hostname, port }
    }

    /// HTTPのHostヘッダの形式の文字列を`(HostName, Option<port>)`に分解する補助関数
    ///
    /// 例: "example.com" -> (Domain("example.com"), None)
    ///     "example.com:8080" -> (Domain("example.com"), Some(8080))
    pub fn parse_host_header(host: &str) -> Result<(HostName, Option<u16>), Error> {
        let hostname;
        let mut port = None;

        // ポート無しのIPv6アドレスは`:`を含むのにポートが無いので特別扱いする
        if host.starts_with('[') && host.ends_with(']') {
            // ポート無しのIPv6アドレスの場合
            hostname = host;
        } else {
            // 最後の':'で分割する(IPv6アドレスの':'をホスト側に残すため)
            if let Some(host) = host.rsplit_once(':') {
                hostname = host.0;
                port = Some(host.1.parse()?);
            } else {
                hostname = host;
            }
        }

        // ホスト部分は IP なら V4/V6、そうでなければ Domain として解釈する。
        let hostname = HostName::from_str(hostname)?;
        Ok((hostname, port))
    }
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let HostName::V6(_) = &self.hostname {
            // IPv6の場合はURL等と同じく`[v6]:port`形式にする。
            // そうしないと':'がポート区切りと混同されるため。
            f.write_char('[')?;
            f.write_str(&self.hostname.to_string())?;
            f.write_str("]:")?;
            f.write_str(&self.port.to_string())
        } else {
            f.write_str(&self.hostname.to_string())?;
            f.write_char(':')?;
            f.write_str(&self.port.to_string())
        }
    }
}

impl FromStr for SocketAddr {
    type Err = Error;

    /// `host:port`形式のパース
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (hostname, port) = s.rsplit_once(':').ok_or("")?;

        Ok(Self {
            hostname: HostName::from_str(hostname)?,
            port: port.parse()?,
        })
    }
}

impl From<std::net::SocketAddr> for SocketAddr {
    /// stdの同名の型からの変換
    /// hostnameは必ずV4/V6のどちらかになる
    fn from(value: std::net::SocketAddr) -> Self {
        Self {
            hostname: value.ip().into(),
            port: value.port(),
        }
    }
}

impl From<std::net::SocketAddrV4> for SocketAddr {
    /// stdのIPv4アドレス専用型からの変換
    /// hostnameは必ずV4になる
    fn from(value: std::net::SocketAddrV4) -> Self {
        Self {
            hostname: (*value.ip()).into(),
            port: value.port(),
        }
    }
}

impl From<std::net::SocketAddrV6> for SocketAddr {
    /// stdの同名の型からの変換
    /// hostnameは必ずV4/V6のどちらかになる
    /// V4になるのはIPv4射影アドレス(::ffff:a.b.c.d)の場合
    fn from(value: std::net::SocketAddrV6) -> Self {
        Self {
            hostname: (*value.ip()).into(),
            port: value.port(),
        }
    }
}

impl TryFrom<&SocketAddr> for std::net::SocketAddr {
    type Error = Error;

    /// stdの同名の型への変換
    /// hostnameがDomainの場合は失敗する
    fn try_from(value: &SocketAddr) -> Result<Self, Self::Error> {
        Ok(Self::new((&value.hostname).try_into()?, value.port))
    }
}

impl TryFrom<&SocketAddr> for std::net::SocketAddrV4 {
    type Error = Error;

    /// stdのIPv4アドレス専用型への変換
    /// hostnameがV4でないと失敗する
    fn try_from(value: &SocketAddr) -> Result<Self, Self::Error> {
        Ok(Self::new((&value.hostname).try_into()?, value.port))
    }
}

impl TryFrom<&SocketAddr> for std::net::SocketAddrV6 {
    type Error = Error;

    /// stdのIPv6アドレス専用型への変換
    /// hostnameがV6でないと失敗する
    fn try_from(value: &SocketAddr) -> Result<Self, Self::Error> {
        Ok(Self::new((&value.hostname).try_into()?, value.port, 0, 0))
    }
}

/// ホスト名を種類付きで保持する列挙型
#[derive(Clone)]
pub enum HostName {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
    Domain(String),
}

impl HostName {
    /// ドメイン名をDoHで解決し, IPアドレスが入ったHostNameを返す
    ///
    /// - 引数について
    ///     - 既にIPアドレスが入っている場合は自身をそのまま返す
    ///     - 引数`qtype`でAかAAAAの問い合わせ種別を指定する
    /// - 戻り値について
    ///     - クエリでエラーが起こった場合はErr(..)を返す
    ///     - クエリには成功したものの回答がなければ`Ok(None)`
    ///     - 最初に見つかったAかAAAAを`Ok(Some(..))`で返す。
    pub async fn dns_resolve(&self, qtype: QueryType) -> Result<Option<Self>, Error> {
        // IP が既に入っている場合は解決不要、というよりこの関数の想定外なのでエラー。
        let domain = match self {
            Self::Domain(domain) => domain,
            _ => return Ok(Some(self.clone())),
        };

        // DNS クエリを構築する
        let mut query = dns_parser::Builder::new_query(0xabcd, true); // IDは特に意味なし
        query.add_question(domain, false, qtype, QueryClass::IN);
        let query = query.build().map_err(|_| "")?;

        // DoHサーバへ問い合わせる
        let result = doh_query(query).await?;
        let response_body = dns_parser::Packet::parse(&result)?;

        // 回答を先頭から走査し, INクラスのAかAAAAを最初に見つけた時点で返す
        for answer in response_body.answers {
            if answer.cls != dns_parser::Class::IN {
                continue;
            }
            match answer.data {
                RData::A(addr) => {
                    return Ok(Some(addr.0.into()));
                }
                RData::AAAA(addr) => {
                    return Ok(Some(addr.0.into()));
                }
                _ => continue,
            }
        }

        // 一致する回答が無かった場合
        Ok(None)
    }

    /// URL中に含める形式のホスト名文字列を返す
    /// to_string等との違いはIPv6のみ角括弧`[]`で囲む点
    pub fn to_string_url_style(&self) -> String {
        match self {
            Self::V6(v6) => format!("[{}]", v6),
            _ => self.to_string(),
        }
    }

    /// 入っているホスト名がIPアドレスかどうかを返す
    pub fn is_ipaddr(&self) -> bool {
        match self {
            Self::V4(_) => true,
            Self::V6(_) => true,
            Self::Domain(_) => false,
        }
    }
}

impl Display for HostName {
    /// 表示用の文字列を返す
    /// IPv6アドレスは角括弧で囲まないので注意
    /// URL中に含める形式の文字列が欲しい場合は`to_string_url_style()`を使うこと
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V4(v4) => f.write_str(&v4.to_string()),
            Self::V6(v6) => f.write_str(&v6.to_string()),
            Self::Domain(domain) => f.write_str(domain),
        }
    }
}

impl FromStr for HostName {
    type Err = Error;

    /// 文字列をHostNameとして解釈する
    /// IPv6アドレスは角括弧で囲まれた形式でも囲まれていない形式でも受け付ける
    /// IPアドレスとして解釈できればV4/V6, それ以外はDomainとして解釈する
    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        // IPv6アドレスの角括弧付きの形式の場合は角括弧を外す
        if s.starts_with('[') && s.ends_with(']') {
            s = &s[1..(s.len() - 1)];
        }

        if let Ok(ip) = std::net::IpAddr::from_str(s) {
            Ok(ip.into())
        } else {
            Ok(Self::Domain(s.to_string()))
        }
    }
}

impl From<std::net::IpAddr> for HostName {
    /// stdのIPアドレス型からの変換
    fn from(value: std::net::IpAddr) -> Self {
        match value {
            std::net::IpAddr::V4(v4) => v4.into(),
            std::net::IpAddr::V6(v6) => v6.into(),
        }
    }
}

impl From<std::net::Ipv4Addr> for HostName {
    /// stdのIPv4アドレス専用型からの変換
    fn from(value: std::net::Ipv4Addr) -> Self {
        Self::V4(value)
    }
}

impl From<std::net::Ipv6Addr> for HostName {
    /// stdのIPv6アドレス専用型からの変換
    /// IPv4射影アドレス(::ffff:a.b.c.d)の場合はV6ではなくV4として正規化する
    fn from(value: std::net::Ipv6Addr) -> Self {
        match value.to_ipv4_mapped() {
            Some(v4) => Self::V4(v4),
            None => Self::V6(value),
        }
    }
}

impl TryFrom<&HostName> for std::net::IpAddr {
    type Error = Error;

    /// stdのIPアドレス型への変換
    /// Domainの場合はエラーになる
    fn try_from(value: &HostName) -> Result<Self, Self::Error> {
        match value {
            HostName::V4(v4) => Ok(Self::V4(*v4)),
            HostName::V6(v6) => Ok(Self::V6(*v6)),
            _ => Err("".into()),
        }
    }
}

impl TryFrom<&HostName> for std::net::Ipv4Addr {
    type Error = Error;

    /// stdのIPv4アドレス専用型への変換
    /// V4のときだけ成功する
    fn try_from(value: &HostName) -> Result<Self, Self::Error> {
        match value {
            HostName::V4(v4) => Ok(*v4),
            _ => Err("".into()),
        }
    }
}

impl TryFrom<&HostName> for std::net::Ipv6Addr {
    type Error = Error;

    /// stdのIPv6アドレス専用型への変換
    /// V6のときだけ成功する
    fn try_from(value: &HostName) -> Result<Self, Self::Error> {
        match value {
            HostName::V6(v6) => Ok(*v6),
            _ => Err("".into()),
        }
    }
}
