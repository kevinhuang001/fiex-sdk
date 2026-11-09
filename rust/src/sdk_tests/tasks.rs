use super::mock::{Mock, context, reference};
use crate::{
    RpcError,
    context::{RequestContext, TaskContext},
    tasks::TaskStatus,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

#[test]
fn request_exposes_method_and_params() {
    let mock = Arc::new(Mock::new());
    let context = RequestContext::new(
        context(mock),
        "archive.compress".into(),
        json!({"format":"zip"}),
    );
    assert_eq!(context.method(), "archive.compress");
    assert_eq!(context.params()["format"], "zip");
}

#[test]
fn spawned_task_inherits_authorization_and_reports_completion() {
    let mock = Arc::new(Mock::with_bytes(b"task data".to_vec()));
    let context = RequestContext::new(context(mock.clone()), "task.test".into(), Value::Null);
    let handle = context
        .spawn_task(|task: TaskContext| {
            task.progress(1, Some(2));
            let data = task
                .files()
                .read_to_string(&reference())
                .map_err(|error| RpcError::internal(error.to_string()))?;
            assert_eq!(data, "task data");
            task.progress(2, Some(2));
            Ok(Value::Null)
        })
        .unwrap();
    handle.wait().unwrap();
    assert_eq!(handle.status(), TaskStatus::Completed);
    assert_eq!(handle.progress(), (2, Some(2)));
    let state = mock.state();
    assert!(state.task_started.is_some());
    assert_eq!(
        state
            .task_finished
            .as_ref()
            .map(|(_, status)| status.as_str()),
        Some("completed")
    );
    assert!(
        state
            .authorizations
            .iter()
            .any(|value| value.get("taskId").is_some())
    );
}

#[test]
fn task_cancellation_is_cooperative_and_reported() {
    let mock = Arc::new(Mock::new());
    let context = RequestContext::new(context(mock.clone()), "task.cancel".into(), Value::Null);
    let handle = context
        .spawn_task(|task: TaskContext| {
            while !task.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Value::Null)
        })
        .unwrap();
    handle.cancel();
    assert!(handle.wait().is_err());
    assert_eq!(handle.status(), TaskStatus::Cancelled);
    assert_eq!(
        mock.state()
            .task_finished
            .as_ref()
            .map(|(_, status)| status.as_str()),
        Some("cancelled")
    );
}

#[test]
fn task_limit_is_enforced() {
    let mock = Arc::new(Mock::new());
    let context = RequestContext::new(context(mock), "task.limit".into(), Value::Null);
    let mut handles = Vec::new();
    for _ in 0..4 {
        handles.push(
            context
                .spawn_task(|task: TaskContext| {
                    while !task.is_cancelled() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Ok(Value::Null)
                })
                .unwrap(),
        );
    }
    assert!(
        context
            .spawn_task(|_: TaskContext| Ok(Value::Null))
            .is_err()
    );
    for handle in &handles {
        handle.cancel();
    }
    for handle in handles {
        let _ = handle.wait();
    }
}

#[test]
fn locale_is_request_local_and_inherited_by_background_work() {
    let mock = Arc::new(Mock::new());
    let request = crate::i18n::with_locale(crate::i18n::Locale::English, || {
        RequestContext::new(
            context(mock),
            "locale.test".into(),
            json!(["business parameters"]),
        )
    });
    assert_eq!(request.locale(), crate::i18n::Locale::English);
    assert_eq!(request.params(), &json!(["business parameters"]));
    assert_eq!(
        crate::i18n::current_locale(),
        crate::i18n::Locale::SimplifiedChinese
    );
    let task = request
        .spawn_task(|task| {
            assert_eq!(task.locale(), crate::i18n::Locale::English);
            let moved_locale = std::thread::spawn(move || task.locale()).join().unwrap();
            assert_eq!(moved_locale, crate::i18n::Locale::English);
            Ok(Value::Null)
        })
        .unwrap();
    task.wait().unwrap();
}
