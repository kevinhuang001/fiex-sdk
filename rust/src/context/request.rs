use super::{ContextInner, decode_output, process_run, report_error, report_log};
use crate::TaskContext;
use crate::{
    RpcError,
    files::Files,
    http::HttpClient,
    ipc::ProcessResult,
    tasks::{TaskHandle, TaskManager},
    tcp::TcpClient,
};
use fiex_provider_contracts::{ProviderInvocation, ProviderRequestContext};
use serde_json::{Value, json};
use std::any::Any;

/// One host request. Dropping it does not revoke the operation; Core revokes
/// authorization when the operation or its owning task ends.
pub struct RequestContext {
    inner: ContextInner,
    request: ProviderRequestContext,
    params: Value,
    locale: crate::i18n::Locale,
}

impl RequestContext {
    pub(crate) fn diagnostic_context(&self) -> ContextInner {
        self.inner.clone()
    }
    pub(crate) fn enable_diagnostics(mut self) -> Self {
        self.inner.diagnostics = true;
        self
    }
    pub(crate) fn new(inner: ContextInner, method: String, params: Value) -> Self {
        let locale = crate::i18n::current_locale();
        let request = ProviderRequestContext::from_params(method, &params, locale.as_str());
        Self {
            inner,
            request,
            params,
            locale,
        }
    }

    /// The method and payload stay identical; only the authorization changes.
    /// The SDK uses it to serve a stream under a longer-lived session.
    pub(crate) fn derive(&self, inner: ContextInner) -> Self {
        Self {
            inner,
            request: self.request.clone(),
            params: self.params.clone(),
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
        &self.request.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    /// Resources affected by this system event. A custom plugin RPC method
    /// may have no targets; no filesystem or network call is made here.
    pub fn targets(&self) -> &[crate::contracts::ResourceContext] {
        &self.request.targets
    }

    pub fn target(&self) -> Option<&crate::ResourceContext> {
        self.request.target()
    }
    pub fn directory(&self) -> Option<crate::ResourceContext> {
        self.request.directory.clone()
    }

    pub fn files(&self) -> Files {
        self.inner.files()
    }

    pub fn http(&self) -> HttpClient {
        self.inner.http()
    }

    pub fn aria2(&self) -> crate::Aria2 {
        self.inner.aria2()
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

    /// Report a structured diagnostic without treating it as a user-facing error.
    pub fn log(&self, level: &str, message: &str) -> Result<(), RpcError> {
        report_log(&self.inner, level, message)
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
        self.spawn_task_with_label(self.method(), work)
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

impl ProviderInvocation for RequestContext {
    fn request(&self) -> &ProviderRequestContext {
        &self.request
    }

    fn local_service_handle(&self) -> Option<&dyn Any> {
        Some(self)
    }
}
