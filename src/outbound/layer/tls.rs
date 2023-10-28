use super::Layer;
use crate::{Connection, Error};

use std::sync::Arc;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::{rustls, TlsConnector};

static CONNECTOR: Lazy<TlsConnector> = Lazy::new(|| {
    let mut certs = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().unwrap() {
        let _ = certs.add(&rustls::Certificate(cert.0));
    }

    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(certs)
        .with_no_client_auth();

    TlsConnector::from(Arc::new(config))
});

pub struct TlsClient();

impl TlsClient {
    pub fn new() -> Self {
        Self()
    }
}

#[async_trait]
impl Layer for TlsClient {
    async fn wrap<RW>(&self, stream: RW, hostname: &str, _: u16) -> Result<Connection, Error>
    where
        RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        Ok(Box::new(
            CONNECTOR.connect(hostname.try_into()?, stream).await?,
        ))
    }
}
