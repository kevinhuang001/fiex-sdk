//! Host runtime: framed IPC, concurrent request dispatch and shared lifecycle.
//!
//! The reader thread owns the control channel. Request handlers run on worker
//! threads, host writes are serialized by the dispatcher, and slow file or
//! network work never stops the process from accepting another request.
mod dispatcher;
mod lifecycle;
pub(crate) mod session;
pub(crate) mod transfer;

use crate::{
    PROTOCOL_VERSION, RpcError,
    context::{ContextInner, RequestContext},
    tasks::TaskManager,
};
use serde_json::{Value, json};
use std::{
    io::{self, BufReader},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
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
/// The object is merged into the handshake reply, so `{"vfs": {...}}` becomes a
/// top-level `vfs` field Core can read before the first business request.
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
        if let Ok(mut reader) = self.reader.lock()
            && let Some(handle) = reader.take()
        {
            let _ = handle.join();
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
}

impl HostLink for Runtime {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.dispatcher.request(method, params)
    }

    fn closed(&self) -> bool {
        self.dispatcher.is_closed()
    }
}
