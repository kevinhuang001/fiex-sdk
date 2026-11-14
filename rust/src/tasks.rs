//! Explicit background tasks with independent lifecycles.
//!
//! A task inherits the operation's approved scope but not its request lifetime.
//! Core keeps the scope alive until the task reports completion, cancellation
//! or failure, then reclaims its file handles, sockets and shared memory.
use crate::TaskContext;
use crate::{RpcError, context::ContextInner, runtime::session::Session};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

const COMPLETED_HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

mod handle;
mod record;
pub use handle::TaskHandle;
pub(crate) use record::TaskRecord;

struct ManagerState {
    next: u64,
    tasks: HashMap<String, Arc<TaskRecord>>,
    finished: VecDeque<String>,
}

pub(crate) struct TaskManager {
    state: Mutex<ManagerState>,
    limit: usize,
}

impl TaskManager {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            state: Mutex::new(ManagerState {
                next: 0,
                tasks: HashMap::new(),
                finished: VecDeque::new(),
            }),
            limit,
        }
    }

    pub(crate) fn spawn<F>(
        parent: &ContextInner,
        label: &str,
        internal: bool,
        locale: crate::i18n::Locale,
        work: F,
    ) -> Result<TaskHandle, RpcError>
    where
        F: FnOnce(TaskContext) -> Result<Value, RpcError> + Send + 'static,
    {
        let id = {
            let mut state = parent
                .tasks()
                .state
                .lock()
                .map_err(|_| RpcError::internal("task registry unavailable"))?;
            if state
                .tasks
                .values()
                .filter(|task| !task.is_finished())
                .count()
                >= parent.tasks().limit
            {
                return Err(RpcError::internal("concurrent task limit reached"));
            }
            state.next += 1;
            let id = format!("task-{}-{}", std::process::id(), state.next);
            let record = Arc::new(TaskRecord::new(id.clone()));
            state.tasks.insert(id.clone(), record);
            id
        };
        let record = parent
            .tasks()
            .state
            .lock()
            .map_err(|_| RpcError::internal("task registry unavailable"))?
            .tasks
            .get(&id)
            .cloned()
            .ok_or_else(|| RpcError::internal("task registration failed"))?;
        if let Err(error) = parent.call(
            "host.task.start",
            json!({"taskId": id, "label": label, "internal": internal}),
        ) {
            if let Ok(mut state) = parent.tasks().state.lock() {
                state.tasks.remove(&id);
            }
            return Err(error);
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let context = parent.fork(Session::Task(id.clone()), Arc::clone(&cancelled));
        let finish = context.clone();
        let task_context = TaskContext::new(context, Arc::clone(&record), locale);
        let record_for_thread = Arc::clone(&record);
        let spawned = thread::Builder::new()
            .name(format!("fiex-sdk-{id}"))
            .spawn(move || {
                let started = std::time::Instant::now();
                if finish.diagnostics {
                    crate::diagnostics::log(&finish, "debug", "background task started");
                }
                let outcome = catch_unwind(AssertUnwindSafe(|| work(task_context)));
                let result = match outcome {
                    Ok(Ok(value)) => Ok(value),
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err(RpcError::internal("task panicked")),
                };
                let status = if record_for_thread.is_cancelled()
                    || cancelled.load(Ordering::Acquire)
                    || result.as_ref().err().is_some_and(|e| e.code == -32800)
                {
                    TaskStatus::Cancelled
                } else if result.is_ok() {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed
                };
                let final_result = match status {
                    TaskStatus::Cancelled => Err(RpcError::cancelled()),
                    _ => result,
                };
                let label = match status {
                    TaskStatus::Completed => "completed",
                    TaskStatus::Cancelled => "cancelled",
                    TaskStatus::Running | TaskStatus::Failed => "failed",
                };
                if finish.diagnostics {
                    crate::diagnostics::log(
                        &finish,
                        if status == TaskStatus::Failed {
                            "error"
                        } else {
                            "info"
                        },
                        &format!(
                            "background task {label}; duration_ms={}",
                            started.elapsed().as_millis()
                        ),
                    );
                }
                let _ = finish.call(
                    "host.task.finish",
                    json!({
                        "taskId": record_for_thread.id(),
                        "status": label,
                        "result": final_result.as_ref().ok(),
                        "error": final_result.as_ref().err().map(|error| error.message.clone()),
                    }),
                );
                record_for_thread.complete(status, final_result);
            });
        if let Err(error) = spawned {
            if let Ok(mut state) = parent.tasks().state.lock() {
                state.tasks.remove(&id);
            }
            let _ = parent.call(
                "host.task.finish",
                json!({"taskId": id, "status": "failed"}),
            );
            return Err(RpcError::internal(format!("could not start task: {error}")));
        }
        let handle = TaskHandle::new(record);
        if let Ok(mut state) = parent.tasks().state.lock() {
            let finished: Vec<String> = state
                .tasks
                .values()
                .filter(|task| task.is_finished())
                .map(|task| task.id().to_string())
                .collect();
            state.finished.extend(finished);
            while state.finished.len() > COMPLETED_HISTORY {
                if let Some(old) = state.finished.pop_front() {
                    state.tasks.remove(&old);
                }
            }
        }
        Ok(handle)
    }

    pub(crate) fn cancel(&self, id: &str) {
        if let Ok(state) = self.state.lock()
            && let Some(task) = state.tasks.get(id)
        {
            task.cancel();
        }
    }

    pub(crate) fn cancel_all(&self) {
        if let Ok(state) = self.state.lock() {
            for task in state.tasks.values() {
                task.cancel();
            }
        }
    }
}
