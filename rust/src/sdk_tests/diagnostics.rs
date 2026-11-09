use super::mock::{Mock, context};
use crate::{RpcError, context::RequestContext, tasks::TaskStatus};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn request_diagnostics_preserve_errors_and_do_not_copy_credentials_or_pop_modals() {
    let mock = Arc::new(Mock::new());
    let ctx = RequestContext::new(
        context(mock.clone()),
        "connection.validate".into(),
        json!({"password":"SECRET"}),
    );
    let result = crate::diagnostics::request(ctx, |_| Err(RpcError::internal("token=SECRET")));
    assert_eq!(result.unwrap_err().message, "token=SECRET");
    let state = mock.state();
    assert_eq!(state.diagnostics.len(), 1);
    assert_eq!(state.diagnostics[0].0, "host.log.report");
    assert_eq!(state.diagnostics[0].1["authorization"]["operationId"], 7);
    assert!(!state.diagnostics[0].1.to_string().contains("SECRET"));
}

#[test]
fn panel_diagnostics_use_authenticated_sdk_and_validate_levels() {
    let mock = Arc::new(Mock::new());
    let ctx = RequestContext::new(
        context(mock.clone()),
        "diagnostics.log".into(),
        json!({"level":"info", "message":"panel ready", "pluginId":"forged"}),
    );
    crate::diagnostics::dispatch(&ctx).unwrap().unwrap();
    let state = mock.state();
    assert_eq!(state.diagnostics[0].1["authorization"]["operationId"], 7);
    assert!(state.diagnostics[0].1.get("pluginId").is_none());
    drop(state);
    let ctx = RequestContext::new(
        context(mock),
        "diagnostics.log".into(),
        json!({"level":"fatal", "message":"bad"}),
    );
    assert!(crate::diagnostics::dispatch(&ctx).unwrap().is_err());
}

#[test]
fn diagnosed_background_tasks_inherit_task_authorization_and_keep_cancellation_quiet() {
    let mock = Arc::new(Mock::new());
    let mut inner = context(mock.clone());
    inner.diagnostics = true;
    let ctx = RequestContext::new(inner, "test.task".into(), Value::Null);
    let task = ctx.spawn_task(|_| Err(RpcError::cancelled())).unwrap();
    assert_eq!(task.wait().unwrap_err().code, -32800);
    assert_eq!(task.status(), TaskStatus::Cancelled);
    let state = mock.state();
    assert_eq!(state.diagnostics.len(), 2);
    assert!(
        state
            .diagnostics
            .iter()
            .all(|(method, p)| method == "host.log.report"
                && p["authorization"]["taskId"].is_string())
    );
    assert!(state.diagnostics.iter().all(|(_, p)| p["level"] != "error"));
}
