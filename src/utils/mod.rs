mod unsplit;
mod uri_parse;

pub use unsplit::UnSplit;
pub use uri_parse::ParsedUri;

use crate::Error;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

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

    let a_to_b = tokio::spawn(async {
        let _ = copy(a.0, b.1).await;
    });
    let b_to_a = tokio::spawn(async {
        let _ = copy(b.0, a.1).await;
    });

    tokio::select! {
        _ = a_to_b => {}
        _ = b_to_a => {}
    }
}
