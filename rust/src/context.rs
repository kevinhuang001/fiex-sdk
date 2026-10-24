//! Request and task contexts. These are the only handles plugins use to reach
//! host-owned resources; the wire protocol and transfer strategy stay hidden.
use crate::{
    files::Files,
    http::HttpClient,
    ipc::ProcessResult,
    runtime::{session::Session, HostLink},
    tasks::{TaskHandle, TaskManager, TaskRecord},
    tcp::TcpClient,
    RpcError,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Map, Value};
use std::sync::{atomic::AtomicBool, Arc};

/// Shared internals of a request or task context.
#[derive(Clone)]
pub(crate) struct ContextInner {
    link: Arc<dyn HostLink>,
    tasks: Arc<TaskManager>,
    session: Session,
    cancelled: Arc<AtomicBool>,
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
        }
    }

    fn authorized(&self, params: Value) -> Result<Value, RpcError> {
        let mut object = match params {
            Value::Null => Map::new(),
            Value::Object(object) => object,
            _ => {
                return Err(RpcError::invalid_params(
                    "host call parameters must be a JSON object",
                ))
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
        }
    }

    pub(crate) fn files(&self) -> Files {
        Files::new(self.clone())
    }

    pub(crate) fn http(&self) -> HttpClient {
        HttpClient::new(self.clone())
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
    if expired {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("authorization expired for this operation: {message}"),
        )
    } else {
        std::io::Error::other(message)
    }
}

fn decode_output(mut result: ProcessResult) -> Result<ProcessResult, RpcError> {
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

/// One host request. Dropping it does not revoke the operation; Core revokes
/// authorization when the operation or its owning task ends.
pub struct RequestContext {
    inner: ContextInner,
    method: String,
    params: Value,
}

impl RequestContext {
    pub(crate) fn new(inner: ContextInner, method: String, params: Value) -> Self {
        Self {
            inner,
            method,
            params,
        }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    pub fn files(&self) -> Files {
        self.inner.files()
    }

    pub fn http(&self) -> HttpClient {
        self.inner.http()
    }

    pub fn tcp(&self) -> TcpClient {
        self.inner.tcp()
    }

    pub fn invoke(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.inner.call(method, params)
    }

    pub fn report_error(
        &self,
        code: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<(), RpcError> {
        report_error(&self.inner, code, message, detail)
    }

    pub fn process_run(
        &self,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        process_run(&self.inner, command, args, stdin)
    }

    pub fn storage_get(&self, scope: &str, key: &str) -> Result<Value, RpcError> {
        self.inner
            .call("host.storage.get", json!({"scope": scope, "key": key}))
    }

    pub fn storage_set(&self, scope: &str, key: &str, value: Value) -> Result<(), RpcError> {
        self.inner.call(
            "host.storage.set",
            json!({"scope": scope, "key": key, "value": value}),
        )?;
        Ok(())
    }

    pub fn secret_get(&self, scope: &str, key: &str) -> Result<String, RpcError> {
        self.inner
            .call("host.secret.get", json!({"scope": scope, "key": key}))?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| RpcError::internal("secret response is not a string"))
    }

    pub fn secret_set(&self, scope: &str, key: &str, value: &str) -> Result<(), RpcError> {
        self.inner.call(
            "host.secret.set",
            json!({"scope": scope, "key": key, "value": value}),
        )?;
        Ok(())
    }

    /// Start work that owns an independent lifecycle. The task inherits the
    /// operation's approved authorization scope but not its request lifetime.
    pub fn spawn_task<F>(&self, work: F) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<(), RpcError> + Send + 'static,
    {
        TaskManager::spawn(&self.inner, work)
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner
            .cancelled()
            .load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Context passed to background work started with [`RequestContext::spawn_task`].
pub struct TaskContext {
    inner: ContextInner,
    record: Arc<TaskRecord>,
}

impl TaskContext {
    pub(crate) fn new(inner: ContextInner, record: Arc<TaskRecord>) -> Self {
        Self { inner, record }
    }

    pub fn id(&self) -> &str {
        self.record.id()
    }

    pub fn files(&self) -> Files {
        self.inner.files()
    }

    pub fn http(&self) -> HttpClient {
        self.inner.http()
    }

    pub fn tcp(&self) -> TcpClient {
        self.inner.tcp()
    }

    pub fn invoke(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.inner.call(method, params)
    }

    pub fn report_error(
        &self,
        code: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<(), RpcError> {
        report_error(&self.inner, code, message, detail)
    }

    pub fn process_run(
        &self,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        process_run(&self.inner, command, args, stdin)
    }

    pub fn storage_get(&self, scope: &str, key: &str) -> Result<Value, RpcError> {
        self.inner
            .call("host.storage.get", json!({"scope": scope, "key": key}))
    }

    pub fn storage_set(&self, scope: &str, key: &str, value: Value) -> Result<(), RpcError> {
        self.inner.call(
            "host.storage.set",
            json!({"scope": scope, "key": key, "value": value}),
        )?;
        Ok(())
    }

    pub fn secret_get(&self, scope: &str, key: &str) -> Result<String, RpcError> {
        self.inner
            .call("host.secret.get", json!({"scope": scope, "key": key}))?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| RpcError::internal("secret response is not a string"))
    }

    pub fn secret_set(&self, scope: &str, key: &str, value: &str) -> Result<(), RpcError> {
        self.inner.call(
            "host.secret.set",
            json!({"scope": scope, "key": key, "value": value}),
        )?;
        Ok(())
    }

    /// Mark cancellation. Long operations should poll [`TaskContext::is_cancelled`].
    pub fn cancel(&self) {
        self.record.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.record.is_cancelled()
            || self
                .inner
                .cancelled()
                .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Best-effort progress for the host and for [`TaskHandle::progress`].
    pub fn progress(&self, done: u64, total: Option<u64>) {
        self.record.set_progress(done, total);
        let _ = self.inner.call(
            "host.task.progress",
            json!({"taskId": self.record.id(), "done": done, "total": total}),
        );
    }
}

fn report_error(
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

fn process_run(
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
