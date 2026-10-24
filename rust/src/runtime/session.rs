//! Authorization session attached to one request or one background task.
//!
//! The host never infers authorization from the current directory. Every
//! `host.*` call carries the session that owns it, so overlapping requests and
//! tasks cannot borrow each other's granted paths, connections or handles.
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Session {
    Operation(u64),
    Task(String),
}

impl Session {
    pub(crate) fn authorization(&self) -> Value {
        match self {
            Self::Operation(id) => json!({"operationId": id}),
            Self::Task(id) => json!({"taskId": id}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_map_to_wire_authorization() {
        assert_eq!(
            Session::Operation(7).authorization(),
            json!({"operationId": 7})
        );
        assert_eq!(
            Session::Task("task-1".into()).authorization(),
            json!({"taskId": "task-1"})
        );
    }
}
