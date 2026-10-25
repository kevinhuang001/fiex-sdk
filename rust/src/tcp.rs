//! Host-brokered TCP byte stream for non-HTTP protocols.
//!
//! Core creates, authorizes and closes the connection. Plugins receive a
//! standard `Read + Write + Send + 'static` stream without IPC handles,
//! channels or host proxies.
use crate::context::{rpc_to_io, ContextInner};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr},
    sync::Arc,
};

const CHUNK: usize = 256 * 1024;

#[derive(Clone)]
pub struct TcpClient {
    context: ContextInner,
}

impl TcpClient {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self { context }
    }

    pub fn connect(&self, host: &str, port: u16) -> io::Result<TcpStream> {
        let value = self
            .context
            .call("host.net.tcp.open", json!({"host": host, "port": port}))
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
            }),
        })
    }
}

struct Connection {
    context: ContextInner,
    id: String,
    address: SocketAddr,
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
        let value = self
            .connection
            .context
            .call(
                "host.net.tcp.readBytes",
                json!({"stream": self.connection.id, "size": size}),
            )
            .map_err(rpc_to_io)?;
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
        self.connection
            .context
            .call(
                "host.net.tcp.writeBytes",
                json!({
                    "stream": self.connection.id,
                    "base64": STANDARD.encode(&data[..size]),
                }),
            )
            .map_err(rpc_to_io)?;
        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
