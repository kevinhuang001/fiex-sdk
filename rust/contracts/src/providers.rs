//! Canonical typed provider traits. SDK runtime and Core native handlers
//! implement these same interfaces; transport adapters stay outside this crate.
use crate::{
    FsEntry, FsListing, FsOperation, FsRequest, InfoSection, PreviewView,
    PreflightDecision, ProviderError, ProviderInvocation, SearchCapabilities,
    SearchMode, SearchResults, TransferPreflight,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests {
    use super::HookResult;

    #[test]
    fn hook_result_requires_handled_and_rejects_legacy_fields() {
        assert_eq!(
            serde_json::from_value::<HookResult>(serde_json::json!({"handled": true})).unwrap(),
            HookResult { handled: true }
        );
        assert!(serde_json::from_value::<HookResult>(serde_json::json!({})).is_err());
        assert!(
            serde_json::from_value::<HookResult>(serde_json::json!({
                "handled": true,
                "preview": false
            }))
            .is_err()
        );
    }
}

pub trait VfsProvider: Send + Sync {
    fn supports_preflight(&self) -> bool {
        false
    }

    fn preflight(
        &self,
        _ctx: &dyn ProviderInvocation,
        _request: &TransferPreflight,
    ) -> Result<PreflightDecision, ProviderError> {
        Ok(PreflightDecision {
            allowed: true,
            reason: None,
        })
    }

    fn operations(&self) -> &'static [FsOperation];

    fn search_capabilities(&self, _ctx: &dyn ProviderInvocation) -> SearchCapabilities {
        SearchCapabilities::default()
    }

    fn discover(
        &self,
        _ctx: &dyn ProviderInvocation,
    ) -> Result<Vec<FsOperation>, ProviderError> {
        Ok(self.operations().to_vec())
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, ProviderError> {
        Err(unsupported(request.op()))
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, ProviderError> {
        Err(unsupported(request.op()))
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<crate::FsRead, ProviderError> {
        Err(unsupported(request.op()))
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<crate::FsWrite, ProviderError> {
        Err(unsupported(request.op()))
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, ProviderError> {
        Err(unsupported(request.op()))
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, ProviderError> {
        Err(unsupported(request.op()))
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), ProviderError> {
        Err(unsupported(request.op()))
    }

    fn search(&self, request: &FsRequest<'_>) -> Result<FsListing, ProviderError> {
        Err(unsupported(request.op()))
    }
}

pub trait InfoProvider: Send + Sync {
    fn resolve(&self, ctx: &dyn ProviderInvocation) -> Result<Vec<InfoSection>, ProviderError>;
}

pub trait PreviewProvider: Send + Sync {
    fn open(&self, ctx: &dyn ProviderInvocation) -> Result<PreviewView, ProviderError>;
}

pub trait DecorationProvider: Send + Sync {
    fn resolve(&self, ctx: &dyn ProviderInvocation) -> Result<Vec<Value>, ProviderError>;
}

pub trait ContextMenuProvider: Send + Sync {
    fn resolve(&self, ctx: &dyn ProviderInvocation) -> Result<Vec<Value>, ProviderError>;
    fn execute(&self, ctx: &dyn ProviderInvocation) -> Result<Value, ProviderError>;
}

pub trait ConnectionContextMenuProvider: Send + Sync {
    fn resolve(&self, ctx: &dyn ProviderInvocation) -> Result<Vec<Value>, ProviderError>;
    fn execute(&self, ctx: &dyn ProviderInvocation) -> Result<Value, ProviderError>;
}

pub trait SearchProvider: Send + Sync {
    fn supports(&self, mode: SearchMode) -> bool {
        mode == SearchMode::Name
    }

    fn query(&self, ctx: &dyn ProviderInvocation) -> Result<SearchResults, ProviderError>;

    /// Stream native or transport-backed results through one shared provider
    /// contract. The default adapts the existing bounded page API; Core-native
    /// walkers can override it to preserve incremental delivery and cancellation.
    fn query_stream(
        &self,
        ctx: &dyn ProviderInvocation,
        cancelled: &dyn Fn() -> bool,
        emit: &mut dyn FnMut(Value) -> Result<(), ProviderError>,
    ) -> Result<SearchResults, ProviderError> {
        let page = self.query(ctx)?;
        for entry in page.entries {
            if cancelled() {
                return Err(ProviderError::cancelled());
            }
            emit(entry)?;
        }
        Ok(SearchResults {
            entries: Vec::new(),
            truncated: page.truncated,
            next_cursor: page.next_cursor,
        })
    }
}

pub trait HookProvider: Send + Sync {
    fn invoke(&self, ctx: &dyn ProviderInvocation) -> Result<HookResult, ProviderError>;
}

/// Typed response for a first-supported hook invocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookResult {
    pub handled: bool,
}

pub trait ToolbarProvider: Send + Sync {
    fn invoke(&self, ctx: &dyn ProviderInvocation) -> Result<Value, ProviderError>;
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionValidation {
    pub valid: bool,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub credentials: std::collections::BTreeMap<String, String>,
}

impl ConnectionValidation {
    pub fn valid() -> Self {
        Self {
            valid: true,
            credentials: Default::default(),
        }
    }

    pub fn credential(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.credentials.insert(name.into(), value.into());
        self
    }

    pub fn into_value(self) -> Value {
        serde_json::json!(self)
    }
}

pub trait ConnectionProvider: Send + Sync {
    fn validate(&self, ctx: &dyn ProviderInvocation) -> Result<Value, ProviderError> {
        Err(ProviderError::method_not_found(ctx.method()))
    }

    fn open(&self, ctx: &dyn ProviderInvocation) -> Result<Value, ProviderError> {
        Err(ProviderError::method_not_found(ctx.method()))
    }

    fn create(&self, ctx: &dyn ProviderInvocation) -> Result<Value, ProviderError> {
        Err(ProviderError::method_not_found(ctx.method()))
    }
}

fn unsupported(op: FsOperation) -> ProviderError {
    let mut error = ProviderError::method_not_found(op.methods()[0]);
    error.message = format!("Unsupported fs operation: {}", op.as_str());
    error
}
