//! SDK-owned best-effort diagnostics. Never record payloads, results or raw errors.
use crate::{context::ContextInner, RequestContext, RpcError};
use serde_json::{json, Value};
use std::time::Instant;

pub(crate) fn log(context: &ContextInner, level: &str, message: &str) {
    let _ = context.call("host.log.report", json!({"level":level,"message":message}));
}
pub(crate) fn request<F>(ctx: RequestContext, work: F) -> Result<Value, RpcError>
where
    F: FnOnce(RequestContext) -> Result<Value, RpcError>,
{
    let context = ctx.diagnostic_context();
    let method = ctx.method().chars().take(96).collect::<String>();
    let started = Instant::now();
    let result = work(ctx);
    let (level, outcome) = match &result {
        Ok(_) => ("debug", "completed".to_owned()),
        Err(error) if matches!(error.code, -32601 | -32800) => {
            ("debug", format!("expected outcome ({})", error.code))
        }
        Err(error) => ("warn", format!("failed (RPC {})", error.code)),
    };
    // RPC responses belong to the caller. Logging never creates a second modal.
    log(
        &context,
        level,
        &format!(
            "{method} {outcome}; duration_ms={}",
            started.elapsed().as_millis()
        ),
    );
    result
}

/// Typed SDK diagnostics endpoints for panels, using authenticated native context.
pub(crate) fn dispatch(ctx: &RequestContext) -> Option<Result<Value, RpcError>> {
    match ctx.method() {
        "diagnostics.log" => Some((|| {
            let level = ctx.params()["level"]
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("missing log level"))?;
            let message = ctx.params()["message"]
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("missing log message"))?;
            ctx.log(level, message)?;
            Ok(Value::Null)
        })()),
        "diagnostics.report" => Some((|| {
            let message = ctx.params()["message"]
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("missing error message"))?;
            let detail = ctx.params()["detail"].as_str();
            ctx.report_error("ui.failure", message, detail)?;
            Ok(Value::Null)
        })()),
        _ => None,
    }
}
