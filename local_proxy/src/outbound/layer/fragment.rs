use super::Layer;
use crate::{utils::SocketAddr, Connection, Error};

use async_trait::async_trait;

use std::{
    future::Future,
    io::ErrorKind,
    mem,
    pin::{pin, Pin},
    task::{ready, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct Fragment();

impl Fragment {
    pub fn new() -> Self {
        Self()
    }
}

#[async_trait]
impl Layer for Fragment {
    async fn wrap<RW>(&self, stream: RW, _: &SocketAddr) -> Result<Connection, Error>
    where
        RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        Ok(Box::new(FragmentLayer::new(stream)))
    }

    fn is_http_passthrough(&self) -> bool {
        true
    }
}

struct FragmentLayer<RW>
where
    RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    inner: RW,
    state: State,
    timer: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl<RW> FragmentLayer<RW>
where
    RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    fn new(stream: RW) -> Self {
        Self {
            inner: stream,
            state: State::WaitingHeader { buf: Buffer::new() },
            timer: None,
        }
    }
}

impl<RW> AsyncRead for FragmentLayer<RW>
where
    RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.state.is_waiting() {
            if self.timer.is_none() {
                let timer = tokio::time::sleep(Duration::from_secs(1));
                self.timer = Some(Box::pin(timer));
            }
        } else {
            self.timer = None;
        }

        if let Some(timer) = &mut self.timer {
            if pin!(timer).poll(cx).is_ready() {
                self.timer = None;
                self.state.try_into_raw_send();
            }
        }

        if self.state.is_sending_buffer() {
            let _ = self.as_mut().poll_flush(cx)?;
        }

        pin!(&mut self.inner).poll_read(cx, buf)
    }
}

impl<RW> AsyncWrite for FragmentLayer<RW>
where
    RW: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        mut buf_write: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let first_buf_write_len = buf_write.len();

        if self.state.is_sending_buffer() {
            ready!(self.as_mut().poll_flush(cx))?;
        }

        if self.state.is_handshake_ended() {
            return pin!(&mut self.inner).poll_write(cx, buf_write);
        }

        if let State::WaitingHeader { buf } = &mut self.state {
            buf.inner.extend_from_slice(buf_write);
            buf_write = &[];

            'label: {
                if !buf.inner.is_empty() && buf.inner[0] != 0x16 {
                    self.state.try_into_raw_send();
                    break 'label;
                }

                if buf.inner.len() >= 6 {
                    if buf.inner[5] != 0x01 {
                        self.state.try_into_raw_send();
                        break 'label;
                    }

                    buf.ptr = 5;

                    let mut buf_ = Buffer::new();
                    mem::swap(buf, &mut buf_);
                    let header = Header::new(&buf_.inner).unwrap();

                    self.state = State::WaitingMessage { buf: buf_, header };
                }
            }
        }

        if let State::WaitingMessage { buf, header } = &mut self.state {
            buf.inner.extend_from_slice(buf_write);

            if buf.inner.len() >= header.len {
                let mut buf_ = Buffer::new();
                mem::swap(buf, &mut buf_);

                self.state = State::SendingMessage {
                    buf: buf_,
                    header: *header,
                    chunk: None,
                };
            }
        }

        Poll::Ready(Ok(first_buf_write_len))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let Self { inner, state, .. } = &mut *self;

        if let State::SendingMessage { buf, header, chunk } = state {
            let msg_max_size = 1_u16;
            while header.len > buf.ptr {
                if let Some(chunk) = chunk {
                    while chunk.inner.len() > chunk.ptr {
                        ready!(pin!(&mut *inner).poll_flush(cx))?;

                        let send = &chunk.inner[(chunk.ptr)..(chunk.ptr + 1)];
                        let written = ready!(pin!(&mut *inner).poll_write(cx, send)?);
                        if written == 0 {
                            return Poll::Ready(Err(ErrorKind::BrokenPipe.into()));
                        }

                        chunk.ptr += written;
                    }

                    buf.ptr += chunk.inner.len() - (header.base.len() + 2);
                }

                let mut chunk_ = Buffer::new();
                chunk_.inner.extend_from_slice(&header.base);

                let mut end = buf.ptr + msg_max_size as usize;
                if end > header.len {
                    end = header.len;
                }

                let msg = &buf.inner[(buf.ptr)..end];
                let msg_len = (msg.len() as u16).to_be_bytes();
                chunk_.inner.extend_from_slice(&msg_len);
                chunk_.inner.extend_from_slice(msg);

                *chunk = Some(chunk_)
            }

            let mut buf_ = Buffer::new();
            mem::swap(buf, &mut buf_);

            *state = State::SendingRawBuffer { buf: buf_ };
        }

        if let State::SendingRawBuffer { buf } = state {
            while buf.inner.len() > buf.ptr {
                let wrote = ready!(pin!(&mut *inner).poll_write(cx, &buf.inner))?;
                buf.ptr += wrote;
            }

            *state = State::SendingData;
        }

        pin!(&mut *inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        pin!(&mut self.inner).poll_shutdown(cx)
    }
}

enum State {
    WaitingHeader {
        buf: Buffer,
    },
    WaitingMessage {
        buf: Buffer,
        header: Header,
    },
    SendingMessage {
        buf: Buffer,
        header: Header,
        chunk: Option<Buffer>,
    },
    SendingRawBuffer {
        buf: Buffer,
    },
    SendingData,
}

impl State {
    fn is_waiting(&self) -> bool {
        matches!(
            self,
            Self::WaitingHeader { .. } | Self::WaitingMessage { .. }
        )
    }

    fn is_sending_buffer(&self) -> bool {
        matches!(
            self,
            Self::SendingMessage { .. } | Self::SendingRawBuffer { .. }
        )
    }

    fn is_handshake_ended(&self) -> bool {
        matches!(self, Self::SendingData)
    }

    fn try_into_raw_send(&mut self) -> bool {
        let buf = match self {
            State::WaitingHeader { buf } => buf,
            State::WaitingMessage { buf, .. } => buf,
            _ => return false,
        };

        let mut buf_ = Buffer::new();
        mem::swap(buf, &mut buf_);
        buf_.ptr = 0;
        *self = State::SendingRawBuffer { buf: buf_ };

        true
    }
}

#[derive(Clone, Copy)]
struct Header {
    base: [u8; 3],
    len: usize,
}

impl Header {
    fn new(header: &[u8]) -> Option<Self> {
        if header.len() < 5 {
            return None;
        }

        Some(Self {
            base: header[0..=2].try_into().unwrap(),
            len: u16::from_be_bytes(header[3..=4].try_into().unwrap()) as usize + 5,
        })
    }
}

struct Buffer {
    inner: Vec<u8>,
    ptr: usize,
}

impl Buffer {
    fn new() -> Self {
        Self {
            inner: Vec::new(),
            ptr: 0,
        }
    }
}
