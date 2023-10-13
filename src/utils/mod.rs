mod unsplit;

pub use unsplit::UnSplit;

use crate::Error;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub async fn copy<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    mut read: R,
    mut write: W,
) -> Result<(), Error> {
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
