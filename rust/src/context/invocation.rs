use super::{RequestContext, TaskContext};
use crate::{
    ProviderInvocation, RpcError, files::Files, http::HttpClient, ipc::ProcessResult,
    tasks::TaskHandle, tcp::TcpClient,
};
use serde_json::Value;

/// SDK-only convenience methods for shared provider invocations. The common
/// context remains runtime-independent; these accessors recover only the
/// current plugin process's authorized Host services.
pub trait ProviderInvocationExt: ProviderInvocation {
    fn sdk_context(&self) -> &RequestContext {
        self.local_service_handle()
            .and_then(|value| value.downcast_ref::<RequestContext>())
            .expect("SDK Host services are available only in the plugin process")
    }
    fn files(&self) -> Files {
        self.sdk_context().files()
    }
    fn http(&self) -> HttpClient {
        self.sdk_context().http()
    }
    fn aria2(&self) -> crate::Aria2 {
        self.sdk_context().aria2()
    }
    fn tcp(&self) -> TcpClient {
        self.sdk_context().tcp()
    }
    fn invoke(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.sdk_context().invoke(method, params)
    }
    fn open_vfs_view(
        &self,
        contribution_id: &str,
        resource_root: &crate::ResourceRef,
        view_context: Value,
    ) -> Result<String, RpcError> {
        self.sdk_context()
            .open_vfs_view(contribution_id, resource_root, view_context)
    }
    fn locale(&self) -> crate::i18n::Locale {
        self.sdk_context().locale()
    }
    fn process_run_at(
        &self,
        directory: &crate::ResourceRef,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        self.sdk_context()
            .process_run_at(directory, command, args, stdin)
    }
    fn process_run(
        &self,
        command: &str,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<ProcessResult, RpcError> {
        self.sdk_context().process_run(command, args, stdin)
    }
    fn oauth(&self) -> crate::OAuth {
        self.sdk_context().oauth()
    }
    fn storage(&self) -> crate::Storage {
        self.sdk_context().storage()
    }
    fn secrets(&self) -> crate::Secrets {
        self.sdk_context().secrets()
    }
    fn log(&self, level: &str, message: &str) -> Result<(), RpcError> {
        self.sdk_context().log(level, message)
    }
    fn report_error(
        &self,
        code: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<(), RpcError> {
        self.sdk_context().report_error(code, message, detail)
    }
    fn spawn_task<F>(&self, work: F) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<Value, RpcError> + Send + 'static,
    {
        self.sdk_context().spawn_task(work)
    }
    fn spawn_task_with_label<F>(&self, label: &str, work: F) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<Value, RpcError> + Send + 'static,
    {
        self.sdk_context().spawn_task_with_label(label, work)
    }
    fn spawn_internal_task<F>(&self, label: &str, work: F) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<Value, RpcError> + Send + 'static,
    {
        self.sdk_context().spawn_internal_task(label, work)
    }
    fn is_cancelled(&self) -> bool {
        self.sdk_context().is_cancelled()
    }
}

impl<T: ProviderInvocation + ?Sized> ProviderInvocationExt for T {}
