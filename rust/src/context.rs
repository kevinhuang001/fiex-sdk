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
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

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
    targets: Vec<crate::contracts::ResourceContext>,
    locale: crate::i18n::Locale,
}

impl RequestContext {
    pub(crate) fn new(inner: ContextInner, method: String, params: Value) -> Self {
        let targets = params
            .get("targets")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            inner,
            method,
            params,
            targets,
            locale: crate::i18n::current_locale(),
        }
    }

    /// The method and payload stay identical; only the authorization changes.
    /// The SDK uses it to serve a stream under a longer-lived session.
    pub(crate) fn derive(&self, inner: ContextInner) -> Self {
        Self {
            inner,
            method: self.method.clone(),
            params: self.params.clone(),
            targets: self.targets.clone(),
            locale: self.locale,
        }
    }

    pub(crate) fn inner(&self) -> &ContextInner {
        &self.inner
    }

    /// Language captured for this request, separate from business parameters.
    pub fn locale(&self) -> crate::i18n::Locale {
        self.locale
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    /// Resources affected by this system event. A custom plugin RPC method
    /// may have no targets; no filesystem or network call is made here.
    pub fn targets(&self) -> &[crate::contracts::ResourceContext] {
        &self.targets
    }

    pub fn target(&self) -> Option<&crate::ResourceContext> {
        self.targets.first()
    }
    pub fn directory(&self) -> Option<crate::ResourceContext> {
        self.params
            .get("directory")
            .filter(|value| value.is_object())
            .and_then(|value| serde_json::from_value(value.clone()).ok())
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

    /// Open a lightweight view of this plugin's explicit `extensions.vfs`
    /// item. Core checks the current selected-resource scope and returns its URI.
    /// Connections use their Core-assigned instance mount instead.
    pub fn open_vfs_view(
        &self,
        contribution_id: &str,
        resource_root: &crate::ResourceRef,
        view_context: Value,
    ) -> Result<String, RpcError> {
        let result = self.invoke("host.vfs.openView", serde_json::json!({"contributionId":contribution_id,"resourceRoot":resource_root,"viewContext":view_context}))?;
        result
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| RpcError::internal("Core returned no view URI"))
    }

    pub fn report_error(
        &self,
        code: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<(), RpcError> {
        report_error(&self.inner, code, message, detail)
    }

    pub fn process_run_at(
        &self,
        directory: &crate::ResourceRef,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        let value = self.inner.call(
            "host.process.run",
            json!({"command":command,"args":args,"stdin":stdin,"cwd":directory}),
        )?;
        decode_output(
            serde_json::from_value(value).map_err(|error| RpcError::internal(error.to_string()))?,
        )
    }

    pub fn process_run(
        &self,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        process_run(&self.inner, command, args, stdin)
    }

    pub fn oauth(&self) -> crate::OAuth {
        crate::OAuth::new(self.inner.clone())
    }
    pub fn storage(&self) -> crate::Storage {
        crate::Storage::new(self.inner.clone())
    }
    pub fn secrets(&self) -> crate::Secrets {
        crate::Secrets::new(self.inner.clone())
    }

    /// Start work that owns an independent lifecycle. The task inherits the
    /// operation's approved authorization scope but not its request lifetime.
    pub fn spawn_task<F>(&self, work: F) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<Value, RpcError> + Send + 'static,
    {
        self.spawn_task_with_label(&self.method, work)
    }

    /// Same as [`RequestContext::spawn_task`] with a user-facing label for the
    /// status centre instead of the request method.
    pub fn spawn_task_with_label<F>(&self, label: &str, work: F) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<Value, RpcError> + Send + 'static,
    {
        let locale = self.locale();
        TaskManager::spawn(&self.inner, label, false, locale, move |task| {
            crate::i18n::with_locale(locale, || work(task))
        })
    }

    /// Start a background task that is an implementation detail rather than a
    /// user-visible operation. Core keeps its authorization but the status
    /// centre does not create an entry for it.
    pub fn spawn_internal_task<F>(&self, label: &str, work: F) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<Value, RpcError> + Send + 'static,
    {
        let locale = self.locale();
        TaskManager::spawn(&self.inner, label, true, locale, move |task| {
            crate::i18n::with_locale(locale, || work(task))
        })
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
    locale: crate::i18n::Locale,
}

impl TaskContext {
    pub fn locale(&self) -> crate::i18n::Locale {
        self.locale
    }
    pub(crate) fn new(
        inner: ContextInner,
        record: Arc<TaskRecord>,
        locale: crate::i18n::Locale,
    ) -> Self {
        Self {
            inner,
            record,
            locale,
        }
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

    pub fn process_run_at(
        &self,
        directory: &crate::ResourceRef,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        let value = self.inner.call(
            "host.process.run",
            json!({"command":command,"args":args,"stdin":stdin,"cwd":directory}),
        )?;
        decode_output(
            serde_json::from_value(value).map_err(|error| RpcError::internal(error.to_string()))?,
        )
    }

    pub fn process_run(
        &self,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        process_run(&self.inner, command, args, stdin)
    }

    pub fn oauth(&self) -> crate::OAuth {
        crate::OAuth::new(self.inner.clone())
    }
    pub fn storage(&self) -> crate::Storage {
        crate::Storage::new(self.inner.clone())
    }
    pub fn secrets(&self) -> crate::Secrets {
        crate::Secrets::new(self.inner.clone())
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
