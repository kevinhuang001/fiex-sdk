//! Fiex native plugin IPC SDK. The public wire format is independent of Rust.
//! Control channel: Content-Length framed JSON-RPC 2.0 over stdin/stdout.
//! Never use stdout for logging; stderr is available for diagnostics.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub mod ipc;

use std::io::{self, BufRead, BufReader, Write};

pub const PROTOCOL_VERSION: &str = "2.0";
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
        let count = reader.read_line(&mut line)?;
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
            let n = value
                .trim()
                .parse::<usize>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid length"))?;
            if n > MAX_FRAME {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "frame too large",
                ));
            }
            length = Some(n);
        }
    }
    let size =
        length.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing length"))?;
    let mut bytes = vec![0u8; size];
    reader.read_exact(&mut bytes)?;
    let value = serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
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

/// A scoped host API client that is usable only while handling one IPC request.
/// Host calls are processed synchronously on the same framed stdio connection.
/// The host binds the request to the authenticated plugin process; no plugin
/// identifier is accepted from a request body.
pub trait HostServices {
    fn invoke(&mut self, method: &str, params: Value) -> Result<Value, RpcError>;
    fn current_directory(&mut self) -> Result<String, RpcError> {
        Ok(self
            .invoke("host.process.cwd", json!({}))?
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    }
    fn buffer_create(
        &mut self,
        size: usize,
        writable: bool,
    ) -> Result<crate::ipc::BufferDescriptor, RpcError> {
        let value = self.invoke(
            "host.buffer.create",
            json!({"size": size, "writable": writable}),
        )?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid buffer descriptor: {error}")))
    }
    fn buffer_map(&mut self, id: &str) -> Result<crate::ipc::BufferDescriptor, RpcError> {
        let value = self.invoke("host.buffer.map", json!({"id": id}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid buffer descriptor: {error}")))
    }
    fn buffer_release(&mut self, id: &str) -> Result<(), RpcError> {
        self.invoke("host.buffer.release", json!({"id": id}))?;
        Ok(())
    }
    fn fs_list(&mut self, path: &str) -> Result<Vec<crate::ipc::Entry>, RpcError> {
        let value = self.invoke("host.fs.list", json!({"path": path}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid directory listing: {error}")))
    }
    fn fs_stat(&mut self, path: &str) -> Result<crate::ipc::Entry, RpcError> {
        let value = self.invoke("host.fs.stat", json!({"path": path}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid file stat: {error}")))
    }
    fn fs_read(
        &mut self,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<crate::ipc::BufferDescriptor, RpcError> {
        let value = self.invoke(
            "host.fs.read",
            json!({"path": path, "offset": offset, "length": length}),
        )?;
        let buffer = value
            .get("buffer")
            .cloned()
            .ok_or_else(|| RpcError::internal("missing read buffer"))?;
        serde_json::from_value(buffer)
            .map_err(|error| RpcError::internal(format!("invalid read buffer: {error}")))
    }
    fn fs_create(&mut self, path: &str) -> Result<crate::ipc::Entry, RpcError> {
        let value = self.invoke("host.fs.create", json!({"path": path}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid created entry: {error}")))
    }
    fn fs_write(
        &mut self,
        path: &str,
        buffer: &str,
        mode: &str,
    ) -> Result<crate::ipc::Entry, RpcError> {
        let value = self.invoke(
            "host.fs.write",
            json!({"path": path, "buffer": buffer, "mode": mode}),
        )?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid written entry: {error}")))
    }
    fn fs_remove(&mut self, path: &str) -> Result<(), RpcError> {
        self.invoke("host.fs.remove", json!({"path": path}))?;
        Ok(())
    }
    fn fs_rename(&mut self, from: &str, to: &str) -> Result<(), RpcError> {
        self.invoke("host.fs.rename", json!({"from": from, "to": to}))?;
        Ok(())
    }
    fn process_run(
        &mut self,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<crate::ipc::ProcessResult, RpcError> {
        let value = self.invoke(
            "host.process.run",
            json!({"command": command, "args": args, "stdin": stdin}),
        )?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid process result: {error}")))
    }
    fn storage_get(&mut self, scope: &str, key: &str) -> Result<Value, RpcError> {
        self.invoke("host.storage.get", json!({"scope":scope,"key":key}))
    }
    fn storage_set(&mut self, scope: &str, key: &str, value: Value) -> Result<(), RpcError> {
        self.invoke(
            "host.storage.set",
            json!({"scope":scope,"key":key,"value":value}),
        )?;
        Ok(())
    }
    fn secret_get(&mut self, scope: &str, key: &str) -> Result<String, RpcError> {
        self.invoke("host.secret.get", json!({"scope":scope,"key":key}))?
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| RpcError::internal("secret response not string"))
    }
    fn secret_set(&mut self, scope: &str, key: &str, value: &str) -> Result<(), RpcError> {
        self.invoke(
            "host.secret.set",
            json!({"scope":scope,"key":key,"value":value}),
        )?;
        Ok(())
    }
}
pub struct HostClient<'a, R: BufRead, W: Write> {
    reader: &'a mut R,
    writer: &'a mut W,
    sequence: u64,
}
impl<R: BufRead, W: Write> HostServices for HostClient<'_, R, W> {
    fn invoke(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        if !method.starts_with("host.") {
            return Err(RpcError::invalid_params("expected host method"));
        }
        self.sequence += 1;
        let id = format!("host-{}", self.sequence);
        write_frame(
            self.writer,
            &json!({
                "jsonrpc": "2.0", "id": id, "method": method, "params": params
            }),
        )
        .map_err(|e| RpcError::internal(e.to_string()))?;
        let response = read_frame(self.reader)
            .map_err(|e| RpcError::internal(e.to_string()))?
            .ok_or_else(|| RpcError::internal("host disconnected"))?;
        if response.get("id").and_then(Value::as_str) != Some(id.as_str()) {
            return Err(RpcError::internal("host response ID mismatch"));
        }
        if let Some(error) = response.get("error") {
            return Err(serde_json::from_value(error.clone())
                .unwrap_or_else(|_| RpcError::internal("host returned malformed error")));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| RpcError::internal("host result missing"))
    }
}

/// Same external process protocol for all implementations, including third parties.
/// Native plugin commands can call the host's namespaced storage and credential
/// services through the provided HostClient.
pub fn serve_with_host(
    plugin_id: &str,
    mut dispatch: impl FnMut(&str, Value, &mut dyn HostServices) -> Result<Value, RpcError>,
) -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut initialized = false;
    while let Some(request) = read_frame(&mut input)? {
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let response = match method {
            "plugin.initialize" => {
                let version = params.get("protocolVersion").and_then(Value::as_str);
                if version != Some(PROTOCOL_VERSION) {
                    Err(RpcError::invalid_params("unsupported protocol version"))
                } else {
                    initialized = true;
                    Ok(
                        json!({"pluginId":plugin_id,"protocolVersion":PROTOCOL_VERSION,"ready":true}),
                    )
                }
            }
            "plugin.ping" if initialized => Ok(json!({"alive":true})),
            "plugin.shutdown" if initialized => {
                write_frame(&mut output, &json!({"jsonrpc":"2.0","id":id,"result":null}))?;
                break;
            }
            _ if !initialized => Err(RpcError::invalid_params("initialize required")),
            other => {
                let mut host = HostClient {
                    reader: &mut input,
                    writer: &mut output,
                    sequence: 0,
                };
                dispatch(other, params, &mut host)
            }
        };
        let message = match response {
            Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
            Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
        };
        write_frame(&mut output, &message)?;
    }
    Ok(())
}

/// Convenience wrapper for plugins without host-side service calls.
pub fn serve(
    plugin_id: &str,
    mut dispatch: impl FnMut(&str, Value) -> Result<Value, RpcError>,
) -> io::Result<()> {
    serve_with_host(plugin_id, |method, params, _host| dispatch(method, params))
}

/// Single threaded, deterministic in-memory harness for parser and SDK tests.
#[cfg(test)]
mod tests {
    use super::*;
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
    fn host_storage_round_trip_is_namespaced_and_framed() {
        let mut host_reply = Vec::new();
        write_frame(
            &mut host_reply,
            &json!({"jsonrpc":"2.0","id":"host-1","result":{"cursor":"opaque"}}),
        )
        .unwrap();
        let mut input = BufReader::new(host_reply.as_slice());
        let mut output = Vec::new();
        let mut api = HostClient {
            reader: &mut input,
            writer: &mut output,
            sequence: 0,
        };
        assert_eq!(
            api.storage_get("account-example", "syncCursor").unwrap(),
            json!({"cursor":"opaque"})
        );
        let sent = read_frame(&mut BufReader::new(output.as_slice()))
            .unwrap()
            .unwrap();
        assert_eq!(sent["method"], "host.storage.get");
        assert_eq!(sent["params"]["scope"], "account-example");
        assert_eq!(sent["params"]["key"], "syncCursor");
    }

    #[test]
    fn non_ascii_byte_lengths_are_correct() {
        let mut data = Vec::new();
        write_frame(&mut data, &json!({"name":"你好"})).unwrap();
        let header = String::from_utf8_lossy(&data);
        assert!(header.contains("Content-Length: 17\r\n"));
    }
}
