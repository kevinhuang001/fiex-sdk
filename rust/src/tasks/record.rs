use super::TaskStatus;
use crate::RpcError;
use serde_json::Value;
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct TaskInner {
    status: TaskStatus,
    progress: (u64, Option<u64>),
    result: Option<Result<Value, RpcError>>,
    done: bool,
}

pub(crate) struct TaskRecord {
    id: String,
    inner: Mutex<TaskInner>,
    changed: Condvar,
    cancel: AtomicBool,
}

impl TaskRecord {
    pub(super) fn new(id: String) -> Self {
        Self {
            id,
            inner: Mutex::new(TaskInner {
                status: TaskStatus::Running,
                progress: (0, None),
                result: None,
                done: false,
            }),
            changed: Condvar::new(),
            cancel: AtomicBool::new(false),
        }
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    pub(crate) fn set_progress(&self, done: u64, total: Option<u64>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.progress = (done, total);
        }
    }

    pub(super) fn progress(&self) -> (u64, Option<u64>) {
        self.inner
            .lock()
            .map(|inner| inner.progress)
            .unwrap_or((0, None))
    }

    pub(super) fn status(&self) -> TaskStatus {
        self.inner
            .lock()
            .map(|inner| inner.status)
            .unwrap_or(TaskStatus::Failed)
    }

    pub(super) fn is_finished(&self) -> bool {
        self.inner.lock().map(|inner| inner.done).unwrap_or(true)
    }

    pub(super) fn complete(&self, status: TaskStatus, result: Result<Value, RpcError>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.status = status;
            inner.result = Some(result);
            inner.done = true;
        }
        self.changed.notify_all();
    }

    pub(super) fn wait(&self) -> Result<Value, RpcError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| RpcError::internal("task state unavailable"))?;
        while !inner.done {
            inner = self
                .changed
                .wait(inner)
                .map_err(|_| RpcError::internal("task state unavailable"))?;
        }
        inner
            .result
            .clone()
            .unwrap_or_else(|| Err(RpcError::internal("task result missing")))
    }
}
