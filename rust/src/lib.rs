//! Fiex native plugin SDK.
//!
//! A plugin process stays resident. Every incoming request gets its own
//! [`RequestContext`]; requests run concurrently while one runtime dispatcher
//! owns the framed control channel. Files, HTTP, TCP, shared memory and
//! background tasks all go through the same runtime, so plugin code only deals
//! with ordinary Rust I/O traits and JSON values.
//!
//! The wire format is Content-Length framed JSON-RPC 2.0 over stdin/stdout.
//! Never use stdout for logging; stderr is available for diagnostics.
extern crate self as fiex_plugin_sdk;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};

mod context;
pub mod contracts;
pub mod files;
pub mod filesystem;
pub mod http;
pub mod i18n;
pub mod ipc;
pub mod oauth;
mod providers;
mod runtime;
pub mod secrets;
pub mod storage;
pub mod tasks;
pub mod tcp;

pub use context::{RequestContext, TaskContext};
pub use contracts::{
    EventContext, InfoRow, InfoSection, PackageView, PreviewView, ResourceContext,
    ResourceMetadata, ResourceRef,
};

pub use files::{FileReader, FileWriter, Files};
pub use http::{HttpClient, Request, Response, StreamingBody};
pub use ipc::{Entry, ProcessResult};
pub use oauth::{AuthorizationCode, OAuth, OAuthRequest};
pub use providers::{
    ConnectionProvider, ConnectionValidation, ContextMenuProvider, DecorationProvider,
    HookProvider, InfoProvider, PreviewProvider, Providers, SearchProvider, SearchResults,
    ToolbarProvider,
};
pub use secrets::{SecretError, Secrets};
pub use storage::Storage;
pub use tasks::{TaskHandle, TaskStatus};
pub use tcp::{TcpClient, TcpReadHalf, TcpStream, TcpWriteHalf};

pub const PROTOCOL_VERSION: &str = "5.0";
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
const MAX_HEADER: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Unsupported method: {method}"),
            data: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            code: -32800,
            message: "task cancelled".into(),
            data: None,
        }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RpcError {}

/// Reads one complete frame. EOF is returned only between frames.
pub fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = String::new();
        // Bound the read itself: read_line followed by a length check can
        // allocate unbounded memory before an attacker sends a newline.
        let remaining = MAX_HEADER.saturating_sub(header_bytes);
        let count =
            std::io::Read::take(reader.by_ref(), (remaining + 1) as u64).read_line(&mut line)?;
        if count == 0 {
            if header_bytes == 0 {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete frame header",
            ));
        }
        header_bytes += count;
        if header_bytes > MAX_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "header too large",
            ));
        }
        if line == "\r\n" {
            break;
        }
        if !line.ends_with("\r\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid header terminator",
            ));
        }
        let (name, value) = line
            .trim_end_matches("\r\n")
            .split_once(':')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid header"))?;
        if name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate length",
                ));
            }
            let size = value
                .trim()
                .parse::<usize>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid length"))?;
            if size > MAX_FRAME {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "frame too large",
                ));
            }
            length = Some(size);
        }
    }
    let size =
        length.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing length"))?;
    let mut bytes = vec![0u8; size];
    reader.read_exact(&mut bytes)?;
    let value = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(value))
}

pub fn write_frame(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    write!(writer, "Content-Length: {}\r\n\r\n", bytes.len())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

/// Start the resident plugin process. The callback runs once per request on a
/// worker thread; all requests share the same process and runtime.
///
/// ```no_run
/// use fiex_plugin_sdk::{self as sdk, RpcError};
/// sdk::serve("org.fiex.example", |ctx| match ctx.method() {
///     "example.ping" => Ok(serde_json::json!({"ok": true})),
///     other => Err(RpcError::method_not_found(other)),
/// })
/// .expect("plugin runtime failed");
/// ```
pub fn serve<F>(plugin_id: &str, dispatch: F) -> io::Result<()>
where
    F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
{
    runtime::serve(plugin_id, dispatch)
}

/// Start a plugin that advertises provider capabilities in the
/// `plugin.initialize` reply. Core records the declaration and refuses
/// operations the plugin never claimed.
pub fn serve_declaring<F>(plugin_id: &str, capabilities: Value, dispatch: F) -> io::Result<()>
where
    F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
{
    runtime::serve_declaring(plugin_id, capabilities, dispatch)
}

#[cfg(test)]
mod sdk_tests;

/// Single threaded, deterministic framing tests.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::BufReader;

    #[test]
    fn utf8_round_trip() {
        let value = json!({"jsonrpc":"2.0","id":"中文","result":{"name":"项目会议"}});
        let mut data = Vec::new();
        write_frame(&mut data, &value).unwrap();
        assert_eq!(
            read_frame(&mut BufReader::new(data.as_slice())).unwrap(),
            Some(value)
        );
    }

    #[test]
    fn oversized_frame_is_rejected() {
        let data = format!("Content-Length: {}\r\n\r\n", MAX_FRAME + 1);
        assert!(read_frame(&mut BufReader::new(data.as_bytes())).is_err());
    }

    #[test]
    fn incomplete_body_is_rejected() {
        assert!(read_frame(&mut BufReader::new(
            b"Content-Length: 20\r\n\r\n{}".as_slice()
        ))
        .is_err());
    }

    #[test]
    fn non_ascii_byte_lengths_are_correct() {
        let mut data = Vec::new();
        write_frame(&mut data, &json!({"name":"你好"})).unwrap();
        let header = String::from_utf8_lossy(&data);
        assert!(header.contains("Content-Length: 17\r\n"));
    }
}
