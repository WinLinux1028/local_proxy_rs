mod dns_resolve;
mod unsplit;
mod uri_parse;

use crate::Error;

use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_rustls::{client::TlsStream, rustls};

pub use dns_resolve::dns_resolve;
pub use unsplit::UnSplit;
pub use uri_parse::ParsedUri;

pub async fn copy<R, W>(mut read: R, mut write: W) -> Result<(), Error>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    while let Ok(buf) = read.fill_buf().await {
        if buf.is_empty() {
            break;
        }

        let _ = write.write_all(buf).await;
        let _ = write.flush().await;

        let buf_len = buf.len();
        read.consume(buf_len);
    }

    Ok(())
}

pub async fn copy_bidirectional<R1, W1, R2, W2>(a: UnSplit<R1, W1>, b: UnSplit<R2, W2>)
where
    R1: AsyncBufRead + Unpin + Send + 'static,
    W1: AsyncWrite + Unpin + Send + 'static,
    R2: AsyncBufRead + Unpin + Send + 'static,
    W2: AsyncWrite + Unpin + Send + 'static,
{
    let a = a.split();
    let b = b.split();

    tokio::select! {
        _ = copy(a.0, b.1) => {}
        _ = copy(b.0, a.1) => {}
    }
}

pub async fn tls_connect<RW>(stream: RW, hostname: &str) -> Result<TlsStream<RW>, Error>
where
    RW: AsyncRead + AsyncWrite + Unpin,
{
    let mut certs = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs()? {
        certs.add(&rustls::Certificate(cert.0))?;
    }

    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(certs)
        .with_no_client_auth();
    let config = tokio_rustls::TlsConnector::from(Arc::new(config));

    Ok(config.connect(hostname.try_into()?, stream).await?)
}
