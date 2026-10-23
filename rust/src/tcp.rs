//! Low-level host-brokered TCP byte stream for non-HTTP protocols.
use crate::{worker::HostProxy, HostServices};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use std::{
    io::{self, Read, Write},
    net::SocketAddr,
    sync::Arc,
};
fn error(e: impl ToString) -> io::Error {
    io::Error::other(e.to_string())
}
struct Connection {
    host: HostProxy,
    id: String,
    address: SocketAddr,
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self
            .host
            .invoke("host.net.tcp.close", json!({"stream": self.id}));
    }
}
#[derive(Clone)]
pub struct TcpStream {
    connection: Arc<Connection>,
}
impl TcpStream {
    pub fn connect(mut host: HostProxy, hostname: &str, port: u16) -> io::Result<Self> {
        let value = host
            .invoke("host.net.tcp.open", json!({"host": hostname, "port": port}))
            .map_err(error)?;
        let id = value["stream"]
            .as_str()
            .ok_or_else(|| error("missing TCP handle"))?
            .into();
        let address = value["peerAddress"]
            .as_str()
            .ok_or_else(|| error("missing peer address"))?
            .parse()
            .map_err(error)?;
        Ok(Self {
            connection: Arc::new(Connection { host, id, address }),
        })
    }
    pub fn peer_addr(&self) -> SocketAddr {
        self.connection.address
    }
}
impl Read for TcpStream {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let size = output.len().min(256 * 1024);
        let value = self
            .connection
            .host
            .clone()
            .invoke(
                "host.net.tcp.readBytes",
                json!({"stream": self.connection.id, "size": size}),
            )
            .map_err(error)?;
        let bytes = STANDARD
            .decode(
                value["base64"]
                    .as_str()
                    .ok_or_else(|| error("invalid TCP data"))?,
            )
            .map_err(error)?;
        if bytes.len() > size {
            return Err(error("TCP data exceeds requested size"));
        }
        output[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}
impl Write for TcpStream {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let size = data.len().min(256 * 1024);
        if size == 0 {
            return Ok(0);
        }
        self.connection
            .host
            .clone()
            .invoke(
                "host.net.tcp.writeBytes",
                json!({"stream": self.connection.id, "base64": STANDARD.encode(&data[..size])}),
            )
            .map_err(error)?;
        Ok(size)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
