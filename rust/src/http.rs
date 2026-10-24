//! HTTP convenience API. Core owns connections, redirects and authorization.
use crate::context::{rpc_to_io, ContextInner};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    time::Duration,
};

const READ_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct HttpClient {
    context: ContextInner,
}

impl HttpClient {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self { context }
    }

    pub fn request(&self, method: &str, url: &str) -> Request {
        Request {
            client: self.clone(),
            method: method.into(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: Vec::new(),
            timeout: Duration::from_secs(90),
            allow_invalid_certs: false,
        }
    }

    pub fn get(&self, url: &str) -> Request {
        self.request("GET", url)
    }

    pub fn post(&self, url: &str) -> Request {
        self.request("POST", url)
    }

    pub fn put(&self, url: &str) -> Request {
        self.request("PUT", url)
    }

    pub fn delete(&self, url: &str) -> Request {
        self.request("DELETE", url)
    }
}

pub struct Request {
    client: HttpClient,
    method: String,
    url: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    timeout: Duration,
    allow_invalid_certs: bool,
}

impl Request {
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn body(mut self, body: impl AsRef<[u8]>) -> Self {
        self.body = body.as_ref().to_vec();
        self
    }

    pub fn json<T: Serialize>(mut self, value: &T) -> io::Result<Self> {
        self.body = serde_json::to_vec(value).map_err(io::Error::other)?;
        self.headers
            .insert("content-type".into(), "application/json".into());
        Ok(self)
    }

    pub fn basic_auth(self, username: &str, password: &str) -> Self {
        self.header(
            "authorization",
            format!(
                "Basic {}",
                STANDARD.encode(format!("{username}:{password}"))
            ),
        )
    }

    pub fn bearer_auth(self, token: &str) -> Self {
        self.header("authorization", format!("Bearer {token}"))
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Only use for a connection whose certificate exception the user approved.
    pub fn allow_invalid_certs(mut self, allow: bool) -> Self {
        self.allow_invalid_certs = allow;
        self
    }

    pub fn send(self) -> io::Result<Response> {
        let value = self
            .client
            .context
            .call(
                "host.net.http.open",
                json!({
                    "method": self.method,
                    "url": self.url,
                    "headers": self.headers,
                    "bodyBase64": STANDARD.encode(self.body),
                    "timeoutMs": self.timeout.as_millis().min(120000) as u64,
                    "allowInvalidCerts": self.allow_invalid_certs,
                }),
            )
            .map_err(rpc_to_io)?;
        let stream = value["stream"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing response stream"))?
            .to_owned();
        let status = value["status"]
            .as_u64()
            .ok_or_else(|| io::Error::other("missing HTTP status"))? as u16;
        let headers = serde_json::from_value(value["headers"].clone())
            .map_err(|error| io::Error::other(format!("invalid HTTP headers: {error}")))?;
        Ok(Response {
            client: self.client,
            stream,
            status,
            headers,
            closed: false,
        })
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
