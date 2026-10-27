//! Tokio traits over brokered IO. No OS socket/descriptor is created here.
//! Reads keep completed bytes across cancellation; writes accept an owned,
//! bounded chunk synchronously and flush it in the background. A Pending
//! poll_write never submits that call's bytes, preserving cancellation safety.
use super::{TcpClient, TcpStream, CHUNK};
use std::{
    collections::VecDeque,
    future::Future,
    io::{self, Read, Write},
    net::Shutdown,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    task::JoinHandle,
};

type Work<T> = JoinHandle<io::Result<T>>;
pub struct AsyncTcpStream {
    inner: TcpStream,
    reading: Option<Work<Vec<u8>>>,
    buffered: VecDeque<u8>,
    writing: Option<Work<()>>,
    shutting_down: Option<Work<()>>,
    closed: bool,
}
impl TcpClient {
    pub async fn connect_async(
        &self,
        host: impl Into<String>,
        port: u16,
    ) -> io::Result<AsyncTcpStream> {
        let client = self.clone();
        let host = host.into();
        tokio::task::spawn_blocking(move || client.connect(&host, port))
            .await
            .map_err(io::Error::other)?
            .map(TcpStream::into_async)
    }
}
impl AsyncTcpStream {
    pub(super) fn new(inner: TcpStream) -> Self {
        Self {
            inner,
            reading: None,
            buffered: VecDeque::new(),
            writing: None,
            shutting_down: None,
            closed: false,
        }
    }
    pub fn get_ref(&self) -> &TcpStream {
        &self.inner
    }
}
fn poll_work<T>(work: &mut Option<Work<T>>, cx: &mut Context<'_>) -> Poll<io::Result<T>> {
    let Some(handle) = work.as_mut() else {
        return Poll::Ready(Err(io::Error::other("missing IO operation")));
    };
    match Pin::new(handle).poll(cx) {
        Poll::Pending => Poll::Pending,
        Poll::Ready(result) => {
            work.take();
            Poll::Ready(result.map_err(io::Error::other).and_then(|result| result))
        }
    }
}
impl AsyncRead for AsyncTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if out.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if self.buffered.is_empty() {
            if self.reading.is_none() {
                let mut stream = self.inner.clone();
                let length = out.remaining().min(CHUNK);
                self.reading = Some(tokio::task::spawn_blocking(move || {
                    let mut bytes = vec![0; length];
                    let size = stream.read(&mut bytes)?;
                    bytes.truncate(size);
                    Ok(bytes)
                }));
            }
            match poll_work(&mut self.reading, cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(bytes)) => self.buffered.extend(bytes),
            }
        }
        let size = out.remaining().min(self.buffered.len());
        let (first, second) = self.buffered.as_slices();
        let head = size.min(first.len());
        out.put_slice(&first[..head]);
        out.put_slice(&second[..size - head]);
        self.buffered.drain(..size);
        Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for AsyncTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.closed || self.shutting_down.is_some() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "TCP write half is closed",
            )));
        }
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        match self.as_mut().poll_flush(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {}
        }
        let size = bytes.len().min(CHUNK);
        let owned = bytes[..size].to_vec();
        let mut stream = self.inner.clone();
        self.writing = Some(tokio::task::spawn_blocking(move || {
            stream.write_all(&owned)
        }));
        Poll::Ready(Ok(size))
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.writing.is_none() {
            Poll::Ready(Ok(()))
        } else {
            poll_work(&mut self.writing, cx)
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.closed {
            return Poll::Ready(Ok(()));
        }
        match self.as_mut().poll_flush(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {}
        }
        if self.shutting_down.is_none() {
            let stream = self.inner.clone();
            self.shutting_down = Some(tokio::task::spawn_blocking(move || {
                stream.shutdown(Shutdown::Write)
            }));
        }
        match poll_work(&mut self.shutting_down, cx) {
            Poll::Ready(result) => {
                self.closed = true;
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
