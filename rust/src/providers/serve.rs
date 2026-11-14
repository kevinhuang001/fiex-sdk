use super::*;
use crate::{RequestContext, RpcError};
use serde_json::{Value, json};

impl Providers {
    /// The handshake advertises the ceiling across registered VFS instances.
    /// Every selected provider is checked again by its own discover() result.
    pub fn capabilities(&self) -> Value {
        let mut operations = std::collections::BTreeSet::new();
        let mut preflight_ops = std::collections::BTreeSet::new();
        let mut providers = Vec::new();
        for (id, fs) in self.filesystem.instances() {
            let methods = fs
                .dispatch
                .operations()
                .iter()
                .map(|op| op.as_str())
                .collect::<Vec<_>>();
            let provider_preflight_ops = fs
                .preflight_handlers
                .operations()
                .into_iter()
                .map(|op| op.as_str())
                .collect::<Vec<_>>();
            preflight_ops.extend(provider_preflight_ops.iter().copied());
            operations.extend(methods.iter().copied());
            providers.push(json!({
                "id": id,
                "operations": methods,
                "preflightOps": provider_preflight_ops,
            }));
        }
        if providers.is_empty() {
            Value::Null
        } else {
            json!({"vfs":{
                "discovery":true,
                "preflightOps":preflight_ops,
                "operations":operations,
                "providers":providers,
            }})
        }
    }

    /// Registration errors are fatal, not silent replacement or fallback.
    pub fn serve_with<F>(self, plugin_id: &str, fallback: F) -> std::io::Result<()>
    where
        F: Fn(RequestContext) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        if !self.registration_errors.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                self.registration_errors.join("; "),
            ));
        }
        let capabilities = self.capabilities();
        crate::serve_declaring(plugin_id, capabilities, move |ctx| {
            let run = |ctx: RequestContext| {
                if let Some(result) = crate::diagnostics::dispatch(&ctx) {
                    return result;
                }
                match self.dispatch(&ctx)? {
                    Some(value) => Ok(value),
                    None => fallback(ctx),
                }
            };
            if self.diagnostics && !ctx.method().starts_with("diagnostics.") {
                crate::diagnostics::request(ctx.enable_diagnostics(), run)
            } else {
                run(ctx)
            }
        })
    }

    pub fn serve(self, plugin_id: &str) -> std::io::Result<()> {
        self.serve_with(plugin_id, |ctx| {
            Err(RpcError::method_not_found(ctx.method()))
        })
    }
}
