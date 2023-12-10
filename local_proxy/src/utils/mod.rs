mod addr;
mod dns;
mod http;
mod uri_parse;

use crate::Error;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub use addr::{HostName, SocketAddr};
pub use dns::doh_query;
pub use http::Body;
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

pub async fn copy_bidirectional<RW1, RW2>(a: RW1, b: RW2)
where
    RW1: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    RW2: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let a = tokio::io::split(a);
    let b = tokio::io::split(b);

    tokio::select! {
        _ = copy(BufReader::new(a.0), b.1) => {}
        _ = copy(BufReader::new(b.0), a.1) => {}
    }
}
