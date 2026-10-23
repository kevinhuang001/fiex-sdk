//! HTTP convenience API. Core owns connections, redirects and authorization.
use crate::{HostServices, RpcError};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::{self, Read},
    time::Duration,
};
fn error(e: impl ToString) -> io::Error {
    io::Error::other(e.to_string())
}

pub struct HttpClient<'a> {
    host: RefCell<&'a mut dyn HostServices>,
}
impl dyn HostServices + '_ {
    pub fn http(&mut self) -> HttpClient<'_> {
        HttpClient {
            host: RefCell::new(self),
        }
    }
}
impl<'a> HttpClient<'a> {
    pub fn request(&self, method: &str, url: &str) -> Request<'_, 'a> {
        Request {
            client: self,
            method: method.into(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: Vec::new(),
            timeout: Duration::from_secs(90),
            allow_invalid_certs: false,
        }
    }
    pub fn get(&self, url: &str) -> Request<'_, 'a> {
        self.request("GET", url)
    }
    pub fn post(&self, url: &str) -> Request<'_, 'a> {
        self.request("POST", url)
    }
    pub fn put(&self, url: &str) -> Request<'_, 'a> {
        self.request("PUT", url)
    }
    pub fn delete(&self, url: &str) -> Request<'_, 'a> {
        self.request("DELETE", url)
    }
}
pub struct Request<'h, 'a> {
    client: &'h HttpClient<'a>,
    method: String,
    url: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    timeout: Duration,
    allow_invalid_certs: bool,
}
impl<'h, 'a> Request<'h, 'a> {
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
    pub fn body(mut self, body: impl AsRef<[u8]>) -> Self {
        self.body = body.as_ref().to_vec();
        self
    }
    pub fn json<T: Serialize>(mut self, value: &T) -> io::Result<Self> {
        self.body = serde_json::to_vec(value).map_err(error)?;
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
    pub fn send(self) -> io::Result<Response<'h, 'a>> {
        let value = self.client.host.borrow_mut().invoke("host.net.http.open", json!({"method": self.method, "url": self.url, "headers": self.headers, "bodyBase64": STANDARD.encode(self.body), "timeoutMs": self.timeout.as_millis().min(120000) as u64, "allowInvalidCerts": self.allow_invalid_certs})).map_err(error)?;
        Ok(Response {
            client: self.client,
            stream: value["stream"]
                .as_str()
                .ok_or_else(|| error("missing response stream"))?
                .into(),
            status: value["status"]
                .as_u64()
                .ok_or_else(|| error("missing HTTP status"))? as u16,
            headers: serde_json::from_value(value["headers"].clone()).map_err(error)?,
            closed: false,
        })
    }
}
pub struct Response<'h, 'a> {
    client: &'h HttpClient<'a>,
    stream: String,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    closed: bool,
}
impl Response<'_, '_> {
    pub fn error_for_status(self) -> io::Result<Self> {
        if self.status >= 400 {
            Err(error(format!("HTTP {}", self.status)))
        } else {
            Ok(self)
        }
    }
    pub fn bytes(mut self) -> io::Result<Vec<u8>> {
        let mut value = Vec::new();
        (&mut self)
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut value)?;
        if value.len() > 64 * 1024 * 1024 {
            return Err(error(
                "response exceeds read-all limit; use Read for streaming",
            ));
        }
        Ok(value)
    }
    pub fn text(self) -> io::Result<String> {
        String::from_utf8(self.bytes()?).map_err(error)
    }
    pub fn json<T: DeserializeOwned>(self) -> io::Result<T> {
        serde_json::from_slice(&self.bytes()?).map_err(error)
    }
}
impl Read for Response<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.closed {
            return Ok(0);
        }
        let size = output.len().min(256 * 1024);
        let value = self
            .client
            .host
            .borrow_mut()
            .invoke(
                "host.net.http.read",
                json!({"stream": self.stream, "size": size}),
            )
            .map_err(error)?;
        let bytes = STANDARD
            .decode(
                value["base64"]
                    .as_str()
                    .ok_or_else(|| error("invalid HTTP chunk"))?,
            )
            .map_err(error)?;
        if bytes.len() > size {
            return Err(error("HTTP chunk exceeds request"));
        }
        output[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}
impl Drop for Response<'_, '_> {
    fn drop(&mut self) {
        let _: Result<Value, RpcError> = self
            .client
            .host
            .borrow_mut()
            .invoke("host.net.http.close", json!({"stream": self.stream}));
        self.closed = true;
    }
}
