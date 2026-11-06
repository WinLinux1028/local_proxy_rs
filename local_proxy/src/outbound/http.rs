use super::{ProxyOutBound, ProxyOutBoundDefaultMethods};
use crate::{
    config::ProxyConfig,
    inbound::http::http_proxy::RequestConfig,
    outbound::ProxyStack,
    utils::{Body, SocketAddr},
    Connection, Error,
};

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use http_body_util::Empty;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use std::str::FromStr;

pub struct HttpProxy {
    addr: SocketAddr,
    auth: Option<String>,
}

impl HttpProxy {
    pub fn new(conf: &ProxyConfig) -> Result<Self, Error> {
        // 認証情報を準備する
        let mut auth = String::new();

        // ユーザー名とパスワードをコロン区切りで連結する
        if let Some(user) = &conf.user {
            auth += user;
        }
        auth += ":"; // パスワードがない場合でもコロンは必要
        if let Some(password) = &conf.password {
            auth += password;
        }

        // 1文字を超える場合は認証情報が指定されたと判断する
        let auth = if auth.len() > 1 {
            let base64 = base64::engine::general_purpose::STANDARD;
            Some(base64.encode(&auth))
        } else {
            None
        };

        Ok(Self {
            addr: SocketAddr::from_str(&conf.server)?,
            auth,
        })
    }
}

#[async_trait]
impl ProxyOutBound for HttpProxy {
    /// HTTP CONNECTを使い, 指定した宛先へのTCPトンネルを作る
    async fn connect(
        &self,
        mut proxies: ProxyStack<'_>,
        addr: &SocketAddr,
    ) -> Result<Connection, Error> {
        // プロキシサーバーとの接続を確立する
        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;
        // 接続をHyperに渡す
        let server = TokioIo::new(server);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(server).await?;
        tokio::spawn(conn.with_upgrades());

        // リクエストを構築する
        let addr_str = addr.to_string();
        let mut request = Request::builder()
            .method(Method::CONNECT)
            .uri(&addr_str)
            .header("host", &addr_str)
            .header("connection", "keep-alive")
            .header("proxy-connection", "keep-alive");
        if let Some(auth) = &self.auth {
            request = request.header("proxy-authorization", format!("Basic {}", auth));
        }
        let request = request.body(Empty::<Bytes>::new())?;

        // リクエストを送る
        let response = sender.send_request(request).await?;
        if !response.status().is_success() {
            return Err("".into()); // 2xx以外のレスポンスコードはエラーとする
        }

        // TCPトンネルとして使える状態にして返す
        Ok(Box::new(TokioIo::new(hyper::upgrade::on(response).await?)))
    }

    /// 平文HTTPリクエストをHTTPプロキシ専用の方法で転送する
    async fn http_proxy(
        &self,
        mut proxies: ProxyStack<'_>,
        scheme: &str,
        req_conf: &RequestConfig,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        // HTTP以外は汎用のリクエスト転送処理に任せる
        if scheme != "http" {
            return self.http_proxy_(proxies, scheme, req_conf, request).await;
        }

        // プロキシサーバーとの接続を確立する
        let server = proxies
            .next()
            .ok_or("")?
            .connect(proxies, &self.addr)
            .await?;
        // 接続をHyperに渡す
        let server = TokioIo::new(server);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(server).await?;
        tokio::spawn(conn.with_upgrades());

        // プロキシサーバーには宛先を含む絶対URIを送る
        let uri = Uri::builder()
            .scheme(scheme)
            .authority(request.headers().get("host").ok_or("")?.to_str()?)
            .path_and_query(request.uri().path_and_query().ok_or("")?.as_str())
            .build()?;

        // プロキシサーバーに渡す用にリクエストの内容を少し変更する
        *request.uri_mut() = uri;
        if let Some(auth) = &self.auth {
            request
                .headers_mut()
                .insert("proxy-authorization", format!("Basic {}", auth).parse()?);
        }

        // Upgradeが行われた場合, 双方のストリームを接続する
        let client = hyper::upgrade::on(&mut request);
        let mut response = sender.send_request(request).await?;
        let server = hyper::upgrade::on(&mut response);
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            tokio::spawn(Self::proxy_upgrade(client, server));
        }

        Ok(Body::convert_response(response))
    }
}
