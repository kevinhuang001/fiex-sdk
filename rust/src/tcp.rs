//! Host-brokered TCP byte stream for non-HTTP protocols.
//!
//! Core creates, authorizes and closes the connection. Plugins receive a
//! standard `Read + Write + Send + 'static` stream without IPC handles,
//! channels or host proxies.
#[cfg(feature = "tokio")]
mod asynchronous;
mod deadline;
use crate::context::{rpc_to_io, ContextInner};
#[cfg(feature = "tokio")]
pub use asynchronous::AsyncTcpStream;
use base64::{engine::general_purpose::STANDARD, Engine};
use deadline::Deadline;
use serde_json::json;
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

const CHUNK: usize = 256 * 1024;

#[derive(Clone)]
pub struct TcpClient {
    context: ContextInner,
    deadline: Option<Deadline>,
}

impl TcpClient {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self {
            context,
            deadline: None,
        }
    }

    /// One absolute deadline shared by connect, read and write (including DNS
    /// and broker queue time). It does not reset after each chunk.
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(Deadline::new(deadline));
        self
    }

    pub fn connect(&self, host: &str, port: u16) -> io::Result<TcpStream> {
        let mut params = json!({"host": host, "port": port});
        if let Some(deadline) = self.deadline {
            deadline.apply(&mut params)?;
        }
        let value = self
            .context
            .call("host.net.tcp.open", params)
            .map_err(rpc_to_io)?;
        let id = value["stream"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing TCP handle"))?
            .to_owned();
        let address = value["peerAddress"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing peer address"))?
            .parse()
            .map_err(|error| io::Error::other(format!("invalid peer address: {error}")))?;
        Ok(TcpStream {
            connection: Arc::new(Connection {
                context: self.context.clone(),
                id,
                address,
                deadline: Mutex::new(self.deadline),
                failed: AtomicBool::new(false),
            }),
        })
    }
}

struct Connection {
    context: ContextInner,
    id: String,
    address: SocketAddr,
    deadline: Mutex<Option<Deadline>>,
    failed: AtomicBool,
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.context.closed() {
            let _ = self
                .context
                .call("host.net.tcp.close", json!({"stream": self.id}));
        }
    }
}

#[derive(Clone)]
pub struct TcpStream {
    connection: Arc<Connection>,
}

impl TcpStream {
    pub fn set_deadline(&self, deadline: Option<Instant>) -> io::Result<()> {
        *self
            .connection
            .deadline
            .lock()
            .map_err(|_| io::Error::other("deadline lock poisoned"))? = deadline.map(Deadline::new);
        Ok(())
    }
    fn request(
        &self,
        method: &str,
        mut params: serde_json::Value,
    ) -> io::Result<serde_json::Value> {
        if self.connection.failed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "TCP connection expired after a deadline failure",
            ));
        }
        let result = (|| {
            if let Some(deadline) = *self
                .connection
                .deadline
                .lock()
                .map_err(|_| io::Error::other("deadline lock poisoned"))?
            {
                deadline.apply(&mut params)?;
            }
            self.connection
                .context
                .call(method, params)
                .map_err(rpc_to_io)
        })();
        if result
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::TimedOut)
        {
            self.connection.failed.store(true, Ordering::Release);
        }
        result
    }
    #[cfg(feature = "tokio")]
    pub fn into_async(self) -> AsyncTcpStream {
        AsyncTcpStream::new(self)
    }

    /// Split one Core-brokered connection into independently owned read and write
    /// halves. They can be sent to different threads; dropping one half does
    /// not close the other half's connection.
    pub fn into_split(self) -> (TcpReadHalf, TcpWriteHalf) {
        (TcpReadHalf(self.clone()), TcpWriteHalf(self))
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.connection.address
    }

    /// Half-close either direction without discarding the opposite direction.
    ///
    /// Cloned streams share one Core connection. `Shutdown::Write` sends a
    /// TCP FIN while allowing the remote peer's response to be read.
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let direction = match how {
            Shutdown::Read => "read",
            Shutdown::Write => "write",
            Shutdown::Both => "both",
        };
        self.connection
            .context
            .call(
                "host.net.tcp.shutdown",
                json!({"stream": self.connection.id, "direction": direction}),
            )
            .map_err(rpc_to_io)?;
        Ok(())
    }
}

impl Read for TcpStream {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let size = output.len().min(CHUNK);
        let value = self.request(
            "host.net.tcp.readBytes",
            json!({"stream":self.connection.id,"size":size}),
        )?;
        let bytes = STANDARD
            .decode(
                value["base64"]
                    .as_str()
                    .ok_or_else(|| io::Error::other("invalid TCP data"))?,
            )
            .map_err(io::Error::other)?;
        if bytes.len() > size {
            return Err(io::Error::other("TCP data exceeds requested size"));
        }
        output[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}

impl Write for TcpStream {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let size = data.len().min(CHUNK);
        if size == 0 {
            return Ok(0);
        }
        self.request(
            "host.net.tcp.writeBytes",
            json!({"stream":self.connection.id,"base64":STANDARD.encode(&data[..size])}),
        )?;
        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The read half of a host-brokered TCP connection.
pub struct TcpReadHalf(TcpStream);

impl TcpReadHalf {
    pub fn peer_addr(&self) -> SocketAddr {
        self.0.peer_addr()
    }

    /// Stop receiving, preserving the other half's ability to write.
    pub fn shutdown(&self) -> io::Result<()> {
        self.0.shutdown(Shutdown::Read)
    }
}

impl Read for TcpReadHalf {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.0.read(output)
    }
}

/// The write half of a host-brokered TCP connection.
pub struct TcpWriteHalf(TcpStream);

impl TcpWriteHalf {
    pub fn peer_addr(&self) -> SocketAddr {
        self.0.peer_addr()
    }

    /// Send a TCP FIN without preventing the read half from receiving.
    pub fn shutdown(&self) -> io::Result<()> {
        self.0.shutdown(Shutdown::Write)
    }
}

impl Write for TcpWriteHalf {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0.write(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
