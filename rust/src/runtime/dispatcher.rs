//! Framed JSON-RPC transport and request/response matching.
//!
//! A single writer serializes complete frames. Host replies are matched by a
//! unique request ID, so request worker threads never read the control channel
//! and never steal another request's response.
use crate::{read_frame, write_frame, RpcError};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Mutex,
    },
    time::Duration,
};

const HOST_TIMEOUT: Duration = Duration::from_secs(110);

pub(crate) struct Dispatcher {
    writer: Mutex<Box<dyn Write + Send>>,
    pending: Mutex<HashMap<u64, mpsc::Sender<Result<Value, RpcError>>>>,
    next_id: AtomicU64,
    closed: AtomicBool,
}

impl Dispatcher {
    pub(crate) fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        }
    }

    pub(crate) fn request(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(RpcError::internal("host session is closed"));
        }
        let timeout =
            if method == "host.net.http.open" && params["bodyReadHandle"].as_str().is_some() {
                Duration::from_millis(
                    params["timeoutMs"]
                        .as_u64()
                        .unwrap_or(3_600_000)
                        .clamp(1, 86_400_000)
                        + 5_000,
                )
            } else if method.starts_with("host.net.tcp.") {
                if let Some(deadline) = params["deadlineMs"].as_u64() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    let remaining = (deadline as u128).saturating_sub(now);
                    if remaining == 0 {
                        return Err(RpcError::internal("TCP deadline exceeded"));
                    }
                    Duration::from_millis(remaining.min(125_000) as u64)
                } else {
                    Duration::from_secs(125)
                }
            } else {
                HOST_TIMEOUT
            };
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| RpcError::internal("host request registry unavailable"))?
            .insert(id, sender);
        let sent = {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| RpcError::internal("host writer unavailable"))?;
            write_frame(
                &mut *writer,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": params,
                }),
            )
        };
        if let Err(error) = sent {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            return Err(RpcError::internal(error.to_string()));
        }
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&id);
                }
                Err(RpcError::internal("host request timed out"))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(RpcError::internal("host session disconnected"))
            }
        }
    }

    /// Complete a host response. Unknown or duplicate IDs are ignored.
    pub(crate) fn complete(&self, id: u64, message: Value) {
        let result = if let Some(error) = message.get("error") {
            Err(serde_json::from_value(error.clone())
                .unwrap_or_else(|_| RpcError::internal("host returned a malformed error")))
        } else {
            message
                .get("result")
                .cloned()
                .ok_or_else(|| RpcError::internal("host response is missing its result"))
        };
        if let Ok(mut pending) = self.pending.lock() {
            if let Some(sender) = pending.remove(&id) {
                let _ = sender.send(result);
            }
        }
    }

    pub(crate) fn reply(&self, id: Value, result: Result<Value, RpcError>) {
        let message = match result {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        if let Ok(mut writer) = self.writer.lock() {
            let _ = write_frame(&mut *writer, &message);
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Fail requests that can no longer be answered, then reject new ones.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut pending) = self.pending.lock() {
            for (_, sender) in pending.drain() {
                let _ = sender.send(Err(RpcError::internal("host session is closed")));
            }
        }
    }
}

/// Read one frame from the shared control channel.
pub(crate) fn read_message(reader: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    read_frame(reader)
}
