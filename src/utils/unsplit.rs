use std::pin::pin;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite};

pub struct UnSplit<R, W>(R, W)
where
    R: AsyncRead + Unpin + Sync + Send,
    W: AsyncWrite + Unpin + Sync + Send;

impl<R, W> UnSplit<R, W>
where
    R: AsyncRead + Unpin + Sync + Send,
    W: AsyncWrite + Unpin + Sync + Send,
{
    pub unsafe fn new(read: R, write: W) -> Self {
        Self(read, write)
    }

    pub fn split(self) -> (R, W) {
        (self.0, self.1)
    }
}

impl<R, W> AsyncRead for UnSplit<R, W>
where
    R: AsyncRead + Unpin + Sync + Send,
    W: AsyncWrite + Unpin + Sync + Send,
{
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        pin!(&mut self.0).poll_read(cx, buf)
    }
}

impl<R, W> AsyncBufRead for UnSplit<R, W>
where
    R: AsyncRead + AsyncBufRead + Unpin + Sync + Send,
    W: AsyncWrite + Unpin + Sync + Send,
{
    fn poll_fill_buf(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&[u8]>> {
        unsafe { std::mem::transmute(pin!(&mut self.0).poll_fill_buf(cx)) }
    }

    fn consume(mut self: std::pin::Pin<&mut Self>, amt: usize) {
        pin!(&mut self.0).consume(amt)
    }
}

impl<R, W> AsyncWrite for UnSplit<R, W>
where
    R: AsyncRead + Unpin + Sync + Send,
    W: AsyncWrite + Unpin + Sync + Send,
{
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        pin!(&mut self.1).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        pin!(&mut self.1).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        pin!(&mut self.1).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        pin!(&mut self.1).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.1.is_write_vectored()
    }
}
