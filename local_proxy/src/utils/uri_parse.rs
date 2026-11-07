use crate::{
    utils::{HostName, SocketAddr},
    Error,
};

use hyper::Uri;
use percent_encoding::{percent_decode_str, percent_encode, NON_ALPHANUMERIC};

/// 分解されたURI
/// 各要素を個別に読み替え, 書き換えできる
#[derive(Clone)]
pub struct ParsedUri {
    pub scheme: Option<String>,
    pub user: Option<Vec<u8>>,
    pub password: Option<Vec<u8>>,
    pub hostname: Option<HostName>,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
}

impl TryFrom<Uri> for ParsedUri {
    type Error = Error;

    /// hyper::Uriを構成要素ごとに分解する
    fn try_from(value: Uri) -> Result<Self, Self::Error> {
        let scheme = value.scheme_str();
        let mut user = None;
        let mut password = None;
        let mut hostname = None;
        let mut port = None;
        let mut path = value.path();
        let query = value.query();

        if let Some(authority) = value.authority() {
            let host;

            // authorityの形式は`[user[:password]@]hostname[:port]`
            // 最後の'@'で分割する(`hostname[:port]`の部分には`@`は含まれないため)
            if let Some((auth, host_)) = authority.as_str().rsplit_once('@') {
                host = host_;

                // 最初の`:`で分割する
                if let Some((user_, password_)) = auth.split_once(':') {
                    // percent-encodeされた認証情報を元バイト列に戻す
                    user = Some(percent_decode_str(user_).collect::<Vec<u8>>());
                    // passwordが空文字列でない場合のみSomeにする
                    if !password_.is_empty() {
                        password = Some(percent_decode_str(password_).collect::<Vec<u8>>());
                    }
                } else {
                    // ':'が無ければuserのみ
                    user = Some(percent_decode_str(auth).collect::<Vec<u8>>());
                }
            } else {
                // 認証情報が無い場合, authority全体がhostname[:port]
                host = authority.as_str();
            }

            // hostの部分はHostヘッダと同形式なので, parse_host_headerでパースする
            let (hostname_, port_) = SocketAddr::parse_host_header(host)?;
            hostname = Some(hostname_);
            port = port_;
        }

        // `http://example.com`のようにpathが空のURIを`/`に正規化する
        if path.is_empty() {
            path = "/";
        }

        Ok(ParsedUri {
            scheme: scheme.map(|s| s.to_string()),
            user,
            password,
            hostname,
            port,
            path: path.to_string(),
            query: query.map(|q| q.to_string()),
        })
    }
}

impl TryInto<Uri> for ParsedUri {
    type Error = Error;

    /// hyper::Uriに戻す
    /// round-tripは保証しないが, HTTPリクエストを構築するときに同じ結果になるものが返ることは保証する
    fn try_into(self) -> Result<Uri, Self::Error> {
        let mut uri = Uri::builder();
        if let Some(scheme) = self.scheme {
            uri = uri.scheme(scheme.as_str());
        }

        // 認証情報の部分を組み立てる
        let mut authority = String::new();
        if let Some(user) = self.user {
            // NON_ALPHANUMERICは英数字以外をすべてencodeする指定
            // 結局実際のリクエストではpercent-decodeしてbase64でエンコードするので問題ない
            authority = percent_encode(&user, NON_ALPHANUMERIC).to_string();
        }
        if let Some(password) = self.password {
            authority.push(':');
            authority.push_str(&percent_encode(&password, NON_ALPHANUMERIC).to_string());
        }

        // 認証情報があるなら区切り文字の`@`を付ける
        if !authority.is_empty() {
            authority.push('@');
        }

        // 接続先情報の部分を組み立てる
        if let Some(hostname) = self.hostname {
            // IPv6 は "[::1]" のように括弧付きで出力する。
            authority.push_str(&hostname.to_string_url_style());
        }
        if let Some(port) = self.port {
            authority.push(':');
            authority.push_str(&port.to_string());
        }

        // authorityが存在するならbuilderに渡す
        if !authority.is_empty() {
            uri = uri.authority(authority);
        }

        // pathとqueryを`path?query`の形で結合する
        let mut path_and_query = self.path;
        if let Some(query) = self.query {
            path_and_query.push('?');
            path_and_query.push_str(&query);
        }
        uri = uri.path_and_query(path_and_query);

        Ok(uri.build()?)
    }
}
