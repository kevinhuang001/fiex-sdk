//! Host runtime: framed IPC, concurrent request dispatch and shared lifecycle.
//!
//! The reader thread owns the control channel. Request handlers run on worker
//! threads, host writes are serialized by the dispatcher, and slow file or
//! network work never stops the process from accepting another request.
mod dispatcher;
pub(crate) mod session;
pub(crate) mod transfer;

use crate::{
    context::{ContextInner, RequestContext},
    tasks::TaskManager,
    RpcError, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::{
    io::{self, BufReader},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_REQUESTS: usize = 64;
const MAX_TASKS: usize = 8;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// A transport that accepts authorized host calls from any worker thread.
pub(crate) trait HostLink: Send + Sync {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError>;
    fn closed(&self) -> bool;
}

struct Shutdown {
    requested: bool,
    error: Option<String>,
}

pub(crate) struct Runtime {
    plugin_id: String,
    capabilities: Value,
    dispatcher: Arc<dispatcher::Dispatcher>,
    tasks: Arc<TaskManager>,
    dispatch: Arc<dyn Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync>,
    initialized: AtomicBool,
    shutdown: Mutex<Shutdown>,
    shutdown_changed: Condvar,
    active_requests: Mutex<usize>,
    active_changed: Condvar,
    reader: Mutex<Option<thread::JoinHandle<()>>>,
}

pub(crate) fn serve<F>(plugin_id: &str, dispatch: F) -> io::Result<()>
where
    F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
{
    serve_declaring(plugin_id, Value::Null, dispatch)
}

/// Start a plugin that advertises `capabilities` from `plugin.initialize`.
/// The object is merged into the handshake reply, so `{"fs": {...}}` becomes a
/// top-level `fs` field Core can read before the first business request.
pub(crate) fn serve_declaring<F>(
    plugin_id: &str,
    capabilities: Value,
    dispatch: F,
) -> io::Result<()>
where
    F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
{
    let runtime = Runtime::start(plugin_id, capabilities, dispatch)?;
    runtime.wait()
}

impl Runtime {
    fn start<F>(plugin_id: &str, capabilities: Value, dispatch: F) -> io::Result<Arc<Self>>
    where
        F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        let dispatcher = Arc::new(dispatcher::Dispatcher::new(Box::new(io::stdout())));
        let runtime = Arc::new(Self {
            plugin_id: plugin_id.to_string(),
            capabilities,
            dispatcher,
            tasks: Arc::new(TaskManager::new(MAX_TASKS)),
            dispatch: Arc::new(dispatch),
            initialized: AtomicBool::new(false),
            shutdown: Mutex::new(Shutdown {
                requested: false,
                error: None,
            }),
            shutdown_changed: Condvar::new(),
            active_requests: Mutex::new(0),
            active_changed: Condvar::new(),
            reader: Mutex::new(None),
        });
        let reader_runtime = Arc::clone(&runtime);
        let reader = thread::Builder::new()
            .name("fiex-sdk-ipc".into())
            .spawn(move || reader_runtime.read_loop(BufReader::new(io::stdin())))?;
        *runtime
            .reader
            .lock()
            .map_err(|_| io::Error::other("reader registry unavailable"))? = Some(reader);
        Ok(runtime)
    }

    fn wait(self: &Arc<Self>) -> io::Result<()> {
        {
            let mut state = self
                .shutdown
                .lock()
                .map_err(|_| io::Error::other("shutdown registry unavailable"))?;
            while !state.requested {
                state = self
                    .shutdown_changed
                    .wait(state)
                    .map_err(|_| io::Error::other("shutdown lock poisoned"))?;
            }
        }
        self.tasks.cancel_all();
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        let mut active = self
            .active_requests
            .lock()
            .map_err(|_| io::Error::other("request registry unavailable"))?;
        while *active > 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, _) = self
                .active_changed
                .wait_timeout(active, remaining)
                .map_err(|_| io::Error::other("request lock poisoned"))?;
            active = next;
        }
        drop(active);
        self.dispatcher.close();
        if let Ok(mut reader) = self.reader.lock() {
            if let Some(handle) = reader.take() {
                let _ = handle.join();
            }
        }
        let error = self
            .shutdown
            .lock()
            .map_err(|_| io::Error::other("shutdown registry unavailable"))?
            .error
            .take();
        match error {
            Some(error) => Err(io::Error::other(error)),
            None => Ok(()),
        }
    }

    fn read_loop(self: &Arc<Self>, mut input: BufReader<io::Stdin>) {
        loop {
            match dispatcher::read_message(&mut input) {
                Ok(Some(message)) => {
                    if self.handle_message(message) {
                        break;
                    }
                }
                Ok(None) => {
                    self.request_shutdown(None);
                    break;
                }
                Err(error) => {
                    self.request_shutdown(Some(error.to_string()));
                    break;
                }
            }
        }
    }

    /// Returns true when the control channel should stop reading.
    fn handle_message(self: &Arc<Self>, message: Value) -> bool {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            if let Some(id) = message.get("id").and_then(Value::as_u64) {
                self.dispatcher.complete(id, message);
            }
            return false;
        };
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "plugin.initialize" => {
                let version = params.get("protocolVersion").and_then(Value::as_str);
                if version != Some(PROTOCOL_VERSION) {
                    if let Some(id) = id {
                        self.dispatcher.reply(
                            id,
                            Err(RpcError::invalid_params("unsupported protocol version")),
                        );
                    }
                    self.request_shutdown(Some("unsupported protocol version".into()));
                    return true;
                }
                self.initialized.store(true, Ordering::Release);
                if let Some(id) = id {
                    let mut reply = json!({
                        "pluginId": self.plugin_id,
                        "protocolVersion": PROTOCOL_VERSION,
                        "ready": true,
                        "concurrentRequests": true,
                        "tasks": true,
                    });
                    if let (Some(target), Some(extra)) =
                        (reply.as_object_mut(), self.capabilities.as_object())
                    {
                        for (key, value) in extra {
                            target.insert(key.clone(), value.clone());
                        }
                    }
                    self.dispatcher.reply(id, Ok(reply));
                }
                false
            }
            "plugin.ping" => {
                if let Some(id) = id {
                    let result = if self.initialized.load(Ordering::Acquire) {
                        Ok(json!({"alive": true}))
                    } else {
                        Err(RpcError::invalid_params("initialize required"))
                    };
                    self.dispatcher.reply(id, result);
                }
                false
            }
            "plugin.shutdown" => {
                if let Some(id) = id {
                    self.dispatcher.reply(id, Ok(Value::Null));
                }
                self.request_shutdown(None);
                true
            }
            "plugin.cancel" => {
                if let Some(task) = params.get("taskId").and_then(Value::as_str) {
                    self.tasks.cancel(task);
                }
                false
            }
            _ => {
                if !self.initialized.load(Ordering::Acquire) {
                    if let Some(id) = id {
                        self.dispatcher
                            .reply(id, Err(RpcError::invalid_params("initialize required")));
                    }
                    return false;
                }
                if let Some(id) = message.get("id").and_then(Value::as_u64) {
                    let locale = crate::i18n::Locale::parse(
                        message
                            .get("locale")
                            .and_then(Value::as_str)
                            .unwrap_or("zh-CN"),
                    );
                    self.spawn_request(id, method, params, locale);
                }
                false
            }
        }
    }

    fn spawn_request(
        self: &Arc<Self>,
        id: u64,
        method: &str,
        params: Value,
        locale: crate::i18n::Locale,
    ) {
        {
            let Ok(mut active) = self.active_requests.lock() else {
                return;
            };
            if *active >= MAX_REQUESTS {
                self.dispatcher.reply(
                    json!(id),
                    Err(RpcError::internal("too many concurrent requests")),
                );
                return;
            }
            *active += 1;
        }
        let runtime = Arc::clone(self);
        let method = method.to_string();
        let spawned = thread::Builder::new()
            .name(format!("fiex-sdk-request-{id}"))
            .spawn(move || {
                let context = runtime.context(session::Session::Operation(id));
                let result = catch_unwind(AssertUnwindSafe(|| {
                    crate::i18n::with_locale(locale, || {
                        (runtime.dispatch)(RequestContext::new(context, method, params))
                    })
                }))
                .unwrap_or_else(|_| Err(RpcError::internal("request handler panicked")));
                runtime.dispatcher.reply(json!(id), result);
                runtime.request_finished();
            });
        if let Err(error) = spawned {
            self.dispatcher.reply(
                json!(id),
                Err(RpcError::internal(format!(
                    "could not start request worker: {error}"
                ))),
            );
            self.request_finished();
        }
    }

    fn context(self: &Arc<Self>, session: session::Session) -> ContextInner {
        let link: Arc<dyn HostLink> = self.clone();
        ContextInner::new(
            link,
            Arc::clone(&self.tasks),
            session,
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn request_finished(&self) {
        if let Ok(mut active) = self.active_requests.lock() {
            *active = active.saturating_sub(1);
        }
        self.active_changed.notify_all();
    }

    fn request_shutdown(&self, error: Option<String>) {
        if let Ok(mut state) = self.shutdown.lock() {
            if !state.requested {
                state.requested = true;
                state.error = error;
            }
        }
        self.shutdown_changed.notify_all();
    }
}

impl HostLink for Runtime {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.dispatcher.request(method, params)
    }

    fn closed(&self) -> bool {
        self.dispatcher.is_closed()
    }
}
