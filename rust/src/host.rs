//! Scoped host services and the synchronous IPC client.
use crate::{read_frame, write_frame, RpcError};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// A scoped host API client that is usable only while handling one IPC request.
/// Host calls are processed synchronously on the same framed stdio connection.
/// The host binds the request to the authenticated plugin process; no plugin
/// identifier is accepted from a request body.
pub trait HostServices {
    fn worker_proxy(&self) -> Option<crate::worker::HostProxy> {
        None
    }
    /// Record a failure the host would otherwise never see: background work,
    /// a partial success, or a renderer that kept running after a response.
    /// Synchronous failures should return `Err(RpcError)` instead; Core records
    /// those automatically at the invoke boundary.
    ///
    /// Best effort: the host caps reports per plugin per minute and silently
    /// drops the excess, so this never fails the plugin.
    fn report_error(
        &mut self,
        code: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<(), RpcError> {
        let mut params = json!({"code": code, "message": message});
        if let Some(detail) = detail {
            params["detail"] = json!(detail);
        }
        self.invoke("host.error.report", params)?;
        Ok(())
    }

    fn invoke(&mut self, method: &str, params: Value) -> Result<Value, RpcError>;
    fn http_request(
        &mut self,
        url: &str,
        method: &str,
        headers: &std::collections::BTreeMap<String, String>,
        body: Option<&str>,
    ) -> Result<crate::ipc::HttpResponse, RpcError> {
        self.http_request_with_options(url, method, headers, body, false)
    }
    fn http_request_with_options(
        &mut self,
        url: &str,
        method: &str,
        headers: &std::collections::BTreeMap<String, String>,
        body: Option<&str>,
        allow_invalid_certs: bool,
    ) -> Result<crate::ipc::HttpResponse, RpcError> {
        let value = self.invoke(
            "host.net.http",
            json!({
                "url": url,
                "method": method,
                "headers": headers,
                "bodyBuffer": body,
                "allowInvalidCerts": allow_invalid_certs
            }),
        )?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid HTTP response: {error}")))
    }
    fn tcp_open(&mut self, host: &str, port: u16) -> Result<crate::ipc::TcpStreamHandle, RpcError> {
        let value = self.invoke("host.net.tcp.open", json!({"host": host, "port": port}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid TCP stream: {error}")))
    }
    fn tcp_read(&mut self, stream: &str, size: usize) -> Result<crate::ipc::TcpRead, RpcError> {
        let value = self.invoke("host.net.tcp.read", json!({"stream": stream, "size": size}))?;
        serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid TCP read: {error}")))
    }
    fn tcp_write(&mut self, stream: &str, buffer: &str) -> Result<(), RpcError> {
        self.invoke(
            "host.net.tcp.write",
            json!({"stream": stream, "buffer": buffer}),
        )?;
        Ok(())
    }
    fn tcp_close(&mut self, stream: &str) -> Result<(), RpcError> {
        self.invoke("host.net.tcp.close", json!({"stream": stream}))?;
        Ok(())
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
        let mut result: crate::ipc::ProcessResult = serde_json::from_value(value)
            .map_err(|error| RpcError::internal(format!("invalid process result: {error}")))?;
        use base64::Engine;
        if let Some(encoded) = &result.stdout_base64 {
            result.stdout = String::from_utf8_lossy(
                &base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|e| RpcError::internal(e.to_string()))?,
            )
            .into_owned();
        }
        if let Some(encoded) = &result.stderr_base64 {
            result.stderr = String::from_utf8_lossy(
                &base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|e| RpcError::internal(e.to_string()))?,
            )
            .into_owned();
        }
        Ok(result)
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
    pub(crate) reader: &'a mut R,
    pub(crate) writer: &'a mut W,
    pub(crate) sequence: u64,
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
