use super::TaskRecord;
use super::TaskStatus;
use crate::RpcError;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct TaskHandle {
    record: Arc<TaskRecord>,
}

impl TaskHandle {
    pub(super) fn new(record: Arc<TaskRecord>) -> Self {
        Self { record }
    }

    pub fn id(&self) -> &str {
        self.record.id()
    }

    /// Request cooperative cancellation. The task should poll `is_cancelled`.
    pub fn cancel(&self) {
        self.record.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.record.is_cancelled()
    }

    pub fn is_finished(&self) -> bool {
        self.record.is_finished()
    }

    pub fn status(&self) -> TaskStatus {
        self.record.status()
    }

    pub fn progress(&self) -> (u64, Option<u64>) {
        self.record.progress()
    }

    /// Block until the task completes or is cancelled.
    pub fn wait(&self) -> Result<Value, RpcError> {
        self.record.wait()
    }
}
