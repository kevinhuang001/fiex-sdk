//! aria2-backed streaming downloads. The API mirrors [`crate::http`]: build a
//! GET request, set headers/timeout/connection options, then read the response.
use crate::context::{rpc_to_io, ContextInner};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    sync::Arc,
    time::Duration,
};

type Progress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

#[derive(Clone)]
pub struct Aria2 {
    context: ContextInner,
}

impl Aria2 {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self { context }
    }

    /// Start a GET download. aria2 only downloads; method-specific requests
    /// stay on [`crate::http`].
    pub fn get(&self, url: &str) -> Aria2Request {
        Aria2Request {
            client: self.clone(),
            url: url.into(),
            headers: BTreeMap::new(),
            timeout: Duration::from_secs(300),
            connections: 8,
            split: 8,
            min_split_size: 1024 * 1024,
            allow_invalid_certs: false,
            size: None,
            progress: None,
        }
    }
}

pub struct Aria2Request {
    client: Aria2,
    url: String,
    headers: BTreeMap<String, String>,
    timeout: Duration,
    connections: u32,
    split: u32,
    min_split_size: u64,
    allow_invalid_certs: bool,
    size: Option<u64>,
    progress: Option<Progress>,
}

impl Aria2Request {
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn bearer_auth(self, token: &str) -> Self {
        self.header("authorization", format!("Bearer {token}"))
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn connections(mut self, connections: u32) -> Self {
        self.connections = connections;
        self
    }

    pub fn split(mut self, split: u32) -> Self {
        self.split = split;
        self
    }

    pub fn min_split_size(mut self, bytes: u64) -> Self {
        self.min_split_size = bytes;
        self
    }

    pub fn allow_invalid_certs(mut self, allow: bool) -> Self {
        self.allow_invalid_certs = allow;
        self
    }

    /// Optional size when the caller already knows it; improves progress and
    /// lets the reader verify a short download.
    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn on_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(u64, Option<u64>) + Send + Sync + 'static,
    {
        self.progress = Some(Arc::new(callback));
        self
    }

    pub fn send(self) -> io::Result<Aria2Response> {
        let value = self
            .client
            .context
            .call(
                "host.aria2.open",
                json!({
                    "url": self.url,
                    "headers": self.headers,
                    "connections": self.connections,
                    "split": self.split,
                    "minSplitSize": self.min_split_size,
                    "timeoutMs": self.timeout.as_millis().min(86_400_000) as u64,
                    "allowInvalidCerts": self.allow_invalid_certs,
                    "size": self.size,
                }),
            )
            .map_err(rpc_to_io)?;
        let size = value["size"].as_u64().or(self.size);
        if let Some(progress) = &self.progress {
            progress(0, size);
        }
        Ok(Aria2Response {
            context: self.client.context,
            stream: value["streamId"]
                .as_str()
                .ok_or_else(|| io::Error::other("missing aria2 stream"))?
                .into(),
            size,
            read: 0,
            progress: self.progress,
            closed: false,
        })
    }
}

pub struct Aria2Response {
    context: ContextInner,
    stream: String,
    size: Option<u64>,
    read: u64,
    progress: Option<Progress>,
    closed: bool,
}

impl Aria2Response {
    pub fn size(&self) -> Option<u64> {
        self.size
    }
}

impl Read for Aria2Response {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.closed {
            return Ok(0);
        }
        let size = output.len().min(256 * 1024);
        let value = self
            .context
            .call(
                "host.aria2.read",
                json!({"streamId": self.stream, "size": size}),
            )
            .map_err(rpc_to_io)?;
        let mut value = value;
        loop {
            let bytes = STANDARD
                .decode(
                    value["base64"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("invalid aria2 chunk"))?,
                )
                .map_err(io::Error::other)?;
            if bytes.len() > size {
                return Err(io::Error::other("aria2 chunk exceeds request"));
            }
            if !bytes.is_empty() || value["eof"].as_bool() == Some(true) {
                output[..bytes.len()].copy_from_slice(&bytes);
                self.read += bytes.len() as u64;
                if let Some(progress) = &self.progress {
                    progress(self.read, self.size);
                }
                return Ok(bytes.len());
            }
            // The host returns an empty non-EOF chunk while the engine waits
            // for network data. Keep this read call alive instead of exposing
            // a spurious EOF to Core.
            std::thread::sleep(std::time::Duration::from_millis(25));
            value = self
                .context
                .call(
                    "host.aria2.read",
                    json!({"streamId": self.stream, "size": size}),
                )
                .map_err(rpc_to_io)?;
        }
    }
}

impl Drop for Aria2Response {
    fn drop(&mut self) {
        if !self.closed && !self.context.closed() {
            let _ = self
                .context
                .call("host.aria2.close", json!({"streamId": self.stream}));
        }
        self.closed = true;
    }
}
