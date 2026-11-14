use super::{HttpClient, READ_LIMIT, response_from};
use crate::context::rpc_to_io;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{self, Read},
};

/// A request body produced by the plugin and streamed through Core.
///
/// Chunks are forwarded as they arrive; the whole body is never buffered.
/// [`StreamingBody::finish`] is the only way to obtain the [`Response`].
/// Dropping an unfinished body asks Core to cancel the request; if the plugin
/// session is already gone, Core still fails the upload at its request
/// deadline (the declared `contentLength` is never reached).
pub struct StreamingBody {
    client: HttpClient,
    handle: Option<String>,
    expected: u64,
    written: u64,
    failed: bool,
}

impl StreamingBody {
    pub(super) fn new(client: HttpClient, handle: String, expected: u64) -> Self {
        Self {
            client,
            handle: Some(handle),
            expected,
            written: 0,
            failed: false,
        }
    }

    /// Number of bytes accepted for the request body so far.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Declared total size passed to [`Request::body_stream`].
    pub fn expected(&self) -> u64 {
        self.expected
    }

    /// Forward one chunk. Write every chunk as soon as it is available; a
    /// failed write aborts the request and later calls fail immediately.
    pub fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(handle) = self.handle.clone() else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "streaming body is already finished",
            ));
        };
        if self.failed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "streaming body was aborted by an earlier write error",
            ));
        }
        let length = bytes.len() as u64;
        if self.written + length > self.expected {
            self.cancel();
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "streaming body exceeds declared length ({} + {} > {})",
                    self.written, length, self.expected
                ),
            ));
        }
        if bytes.is_empty() {
            return Ok(());
        }
        if let Err(error) = self.client.context.call(
            "host.net.http.body",
            json!({"handle": handle, "base64": STANDARD.encode(bytes), "eof": false}),
        ) {
            self.failed = true;
            self.cancel();
            return Err(rpc_to_io(error));
        }
        self.written += length;
        Ok(())
    }

    /// Close the body and wait for the response head. The returned response
    /// is the same type as [`Request::send`].
    pub fn finish(mut self) -> io::Result<Response> {
        let Some(handle) = self.handle.clone() else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "streaming body is already finished",
            ));
        };
        if self.failed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "streaming body was aborted by an earlier write error",
            ));
        }
        if self.written != self.expected {
            self.cancel();
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "streaming body is incomplete ({} of {} bytes)",
                    self.written, self.expected
                ),
            ));
        }
        self.client
            .context
            .call(
                "host.net.http.body",
                json!({"handle": handle, "base64": "", "eof": true}),
            )
            .map_err(rpc_to_io)?;
        let value = self
            .client
            .context
            .call("host.net.http.response", json!({"handle": handle}))
            .map_err(rpc_to_io)?;
        // The handle now belongs to the response stream.
        self.handle = None;
        response_from(self.client.clone(), value)
    }

    /// Abort an unfinished body. Best effort: Core drops the request and the
    /// worker fails the streamed body. Idempotent.
    pub fn cancel(&mut self) {
        if let Some(handle) = self.handle.take()
            && !self.client.context.closed()
        {
            let _ = self
                .client
                .context
                .call("host.net.http.close", json!({"stream": handle}));
        }
        self.failed = true;
    }
}

impl Drop for StreamingBody {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.cancel();
        }
    }
}

pub struct Response {
    client: HttpClient,
    stream: String,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    closed: bool,
}

impl Response {
    pub(super) fn new(
        client: HttpClient,
        stream: String,
        status: u16,
        headers: BTreeMap<String, String>,
    ) -> Self {
        Self {
            client,
            stream,
            status,
            headers,
            closed: false,
        }
    }

    pub fn error_for_status(self) -> io::Result<Self> {
        if self.status >= 400 {
            Err(io::Error::other(format!("HTTP {}", self.status)))
        } else {
            Ok(self)
        }
    }

    pub fn bytes(mut self) -> io::Result<Vec<u8>> {
        let mut value = Vec::new();
        (&mut self)
            .take((READ_LIMIT + 1) as u64)
            .read_to_end(&mut value)?;
        if value.len() > READ_LIMIT {
            return Err(io::Error::other(
                "response exceeds read-all limit; use Read for streaming",
            ));
        }
        Ok(value)
    }

    pub fn text(self) -> io::Result<String> {
        String::from_utf8(self.bytes()?).map_err(io::Error::other)
    }

    pub fn json<T: DeserializeOwned>(self) -> io::Result<T> {
        serde_json::from_slice(&self.bytes()?).map_err(io::Error::other)
    }
}

impl Read for Response {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.closed {
            return Ok(0);
        }
        let size = output.len().min(256 * 1024);
        let value = self
            .client
            .context
            .call(
                "host.net.http.read",
                json!({"stream": self.stream, "size": size}),
            )
            .map_err(rpc_to_io)?;
        let bytes = STANDARD
            .decode(
                value["base64"]
                    .as_str()
                    .ok_or_else(|| io::Error::other("invalid HTTP chunk"))?,
            )
            .map_err(io::Error::other)?;
        if bytes.len() > size {
            return Err(io::Error::other("HTTP chunk exceeds request"));
        }
        output[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        if !self.closed && !self.client.context.closed() {
            let _ = self
                .client
                .context
                .call("host.net.http.close", json!({"stream": self.stream}));
        }
        self.closed = true;
    }
}
