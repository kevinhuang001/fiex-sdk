//! Request and task contexts. These are the only handles plugins use to reach
//! host-owned resources; the wire protocol and transfer strategy stay hidden.
use crate::{
    RpcError,
    files::Files,
    http::HttpClient,
    ipc::ProcessResult,
    runtime::{HostLink, session::Session},
    tasks::TaskManager,
    tcp::TcpClient,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Shared internals of a request or task context.
#[derive(Clone)]
pub(crate) struct ContextInner {
    link: Arc<dyn HostLink>,
    tasks: Arc<TaskManager>,
    session: Session,
    cancelled: Arc<AtomicBool>,
    pub(crate) diagnostics: bool,
}

impl ContextInner {
    pub(crate) fn new(
        link: Arc<dyn HostLink>,
        tasks: Arc<TaskManager>,
        session: Session,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            link,
            tasks,
            session,
            cancelled,
            diagnostics: false,
        }
    }

    fn authorized(&self, params: Value) -> Result<Value, RpcError> {
        let mut object = match params {
            Value::Null => Map::new(),
            Value::Object(object) => object,
            _ => {
                return Err(RpcError::invalid_params(
                    "host call parameters must be a JSON object",
                ));
            }
        };
        object.insert("authorization".into(), self.session.authorization());
        Ok(Value::Object(object))
    }

    pub(crate) fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if !method.starts_with("host.") {
            return Err(RpcError::invalid_params("expected a host method"));
        }
        if self.link.closed() {
            return Err(RpcError::internal("host session is closed"));
        }
        self.link.call(method, self.authorized(params)?)
    }

    pub(crate) fn closed(&self) -> bool {
        self.link.closed()
    }

    /// Derive a context with a new authorization session while sharing the
    /// same runtime and task registry.
    pub(crate) fn fork(&self, session: Session, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            link: Arc::clone(&self.link),
            tasks: Arc::clone(&self.tasks),
            session,
            cancelled,
            diagnostics: self.diagnostics,
        }
    }

    pub(crate) fn files(&self) -> Files {
        Files::new(self.clone())
    }

    pub(crate) fn http(&self) -> HttpClient {
        HttpClient::new(self.clone())
    }

    pub(crate) fn aria2(&self) -> crate::Aria2 {
        crate::Aria2::new(self.clone())
    }

    pub(crate) fn tcp(&self) -> TcpClient {
        TcpClient::new(self.clone())
    }

    pub(crate) fn tasks(&self) -> &Arc<TaskManager> {
        &self.tasks
    }

    pub(crate) fn cancelled(&self) -> &Arc<AtomicBool> {
        &self.cancelled
    }
}

/// Authorization that outlives one request, used by file streams.
///
/// Core authorizes every host call with the request that makes it and revokes
/// that authorization as soon as the request returns. A provider's read stream
/// may be backed by host resources (an HTTP response body, a TCP socket, a file
/// handle), and Core pulls those streams with `vfs.readStream` and finishes them
/// with `vfs.commit` in *later* requests, so a stream created under the
/// `vfs.openRead`/`vfs.beginWrite` request would lose its host handles between
/// chunks. The SDK therefore creates each stream under a private background
/// task: Core keeps a task's authorization and handles until the task finishes,
/// which happens when the stream is closed.
pub(crate) struct StreamSession {
    id: String,
    context: ContextInner,
}

static STREAMS: AtomicU64 = AtomicU64::new(0);

impl StreamSession {
    /// Start a session for one stream. `None` when Core refuses (for example at
    /// its background-task limit); the caller then keeps the request session,
    /// which still serves providers whose streams hold no host resources.
    pub(crate) fn open(parent: &ContextInner, label: &str) -> Option<Self> {
        if parent.closed() {
            return None;
        }
        let id = format!(
            "stream-{}-{}",
            std::process::id(),
            STREAMS.fetch_add(1, Ordering::Relaxed) + 1
        );
        parent
            .call(
                "host.task.start",
                json!({"taskId": id, "label": label, "internal": true}),
            )
            .ok()?;
        let context = parent.fork(Session::Task(id.clone()), Arc::new(AtomicBool::new(false)));
        Some(Self { id, context })
    }

    pub(crate) fn context(&self) -> &ContextInner {
        &self.context
    }
}

impl Drop for StreamSession {
    fn drop(&mut self) {
        if self.context.closed() {
            return;
        }
        let _ = self.context.call(
            "host.task.finish",
            json!({"taskId": self.id, "status": "completed"}),
        );
    }
}

/// Convert host failures into ordinary I/O errors. Authorization expiry has a
/// stable, explicit message so plugins can surface it to the user.
pub(crate) fn rpc_to_io(error: RpcError) -> std::io::Error {
    let message = error.message.clone();
    let expired = error.code == -32001
        || message.contains("not selected")
        || message.contains("handle unavailable")
        || message.contains("authorization")
        || message.contains("permission denied")
        || message.contains("outside task directory");
    if message.to_ascii_lowercase().contains("timed out")
        || message.to_ascii_lowercase().contains("deadline")
        || message.to_ascii_lowercase().contains("timeout")
    {
        std::io::Error::new(std::io::ErrorKind::TimedOut, message)
    } else if expired {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("authorization expired for this operation: {message}"),
        )
    } else {
        std::io::Error::other(message)
    }
}

pub(crate) fn decode_output(mut result: ProcessResult) -> Result<ProcessResult, RpcError> {
    if let Some(encoded) = &result.stdout_base64 {
        result.stdout = String::from_utf8_lossy(
            &STANDARD
                .decode(encoded)
                .map_err(|error| RpcError::internal(error.to_string()))?,
        )
        .into_owned();
    }
    if let Some(encoded) = &result.stderr_base64 {
        result.stderr = String::from_utf8_lossy(
            &STANDARD
                .decode(encoded)
                .map_err(|error| RpcError::internal(error.to_string()))?,
        )
        .into_owned();
    }
    Ok(result)
}

pub(crate) fn report_log(inner: &ContextInner, level: &str, message: &str) -> Result<(), RpcError> {
    if !matches!(level, "trace" | "debug" | "info" | "warn" | "error") {
        return Err(RpcError::invalid_params("invalid log level"));
    }
    inner.call("host.log.report", json!({"level":level,"message":message}))?;
    Ok(())
}

pub(crate) fn report_error(
    inner: &ContextInner,
    code: &str,
    message: &str,
    detail: Option<&str>,
) -> Result<(), RpcError> {
    let mut params = json!({"code": code, "message": message});
    if let Some(detail) = detail {
        params["detail"] = json!(detail);
    }
    inner.call("host.error.report", params)?;
    Ok(())
}

pub(crate) fn process_run(
    inner: &ContextInner,
    command: &str,
    args: &[String],
    stdin: Option<&str>,
) -> Result<ProcessResult, RpcError> {
    let value = inner.call(
        "host.process.run",
        json!({"command": command, "args": args, "stdin": stdin}),
    )?;
    let result: ProcessResult = serde_json::from_value(value)
        .map_err(|error| RpcError::internal(format!("invalid process result: {error}")))?;
    decode_output(result)
}

mod invocation;
mod request;
mod task;
pub use invocation::ProviderInvocationExt;
pub use request::RequestContext;
pub use task::TaskContext;
