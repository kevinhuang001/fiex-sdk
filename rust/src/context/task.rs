use super::{ContextInner, decode_output, process_run, report_error, report_log};
use crate::{
    RpcError, files::Files, http::HttpClient, ipc::ProcessResult, tasks::TaskRecord, tcp::TcpClient,
};
use serde_json::{Value, json};
use std::sync::Arc;

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

    pub fn aria2(&self) -> crate::Aria2 {
        self.inner.aria2()
    }

    pub fn tcp(&self) -> TcpClient {
        self.inner.tcp()
    }

    pub fn invoke(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.inner.call(method, params)
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
