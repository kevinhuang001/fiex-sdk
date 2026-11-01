//! Canonical typed provider traits. SDK runtime and Core native handlers
//! implement these same interfaces; transport adapters stay outside this crate.
use crate::{
    ConnectionOpenResult, DecorationContribution, FsEntry, FsListing, FsOperation, FsRequest,
    InfoSection, MenuExecuteResult, MenuItem, PreviewView,
    PreflightDecision, ProviderError, ProviderInvocation, SearchCapabilities,
    SearchEntry, SearchMode, SearchResults, TransferPreflight,
};
use serde::{Deserialize, Serialize};

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

    /// Check one operation at the provider boundary. `create` indicates that
    /// the target may not exist yet; storage providers can authorize its
    /// parent directory without manufacturing a node entry.
    fn authorize(
        &self,
        ctx: &dyn ProviderInvocation,
        operation: FsOperation,
        _create: bool,
    ) -> Result<(), ProviderError> {
        if self.discover(ctx)?.contains(&operation) {
            Ok(())
        } else {
            Err(unsupported(operation))
        }
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

    /// Preserve Core cancellation when a native backend has a bounded walker.
    /// Transport-backed providers can use the default request-level check.
    fn search_cancellable(
        &self,
        request: &FsRequest<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<FsListing, ProviderError> {
        if cancelled() {
            return Err(ProviderError::cancelled());
        }
        self.search(request)
    }
}

pub trait InfoProvider: Send + Sync {
    fn resolve(&self, ctx: &dyn ProviderInvocation) -> Result<Vec<InfoSection>, ProviderError>;
}

pub trait PreviewProvider: Send + Sync {
    fn open(&self, ctx: &dyn ProviderInvocation) -> Result<PreviewView, ProviderError>;
}

pub trait DecorationProvider: Send + Sync {
    fn resolve(
        &self,
        ctx: &dyn ProviderInvocation,
    ) -> Result<Vec<DecorationContribution>, ProviderError>;
}

pub trait ContextMenuProvider: Send + Sync {
    fn resolve(&self, ctx: &dyn ProviderInvocation) -> Result<Vec<MenuItem>, ProviderError>;
    fn execute(&self, ctx: &dyn ProviderInvocation) -> Result<MenuExecuteResult, ProviderError>;
}

pub trait ConnectionContextMenuProvider: Send + Sync {
    fn resolve(&self, ctx: &dyn ProviderInvocation) -> Result<Vec<MenuItem>, ProviderError>;
    fn execute(&self, ctx: &dyn ProviderInvocation) -> Result<MenuExecuteResult, ProviderError>;
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
        emit: &mut dyn FnMut(SearchEntry) -> Result<(), ProviderError>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookResult {
    pub handled: bool,
}

pub trait HookProvider: Send + Sync {
    fn invoke(&self, ctx: &dyn ProviderInvocation) -> Result<HookResult, ProviderError>;
}

/// Toolbar invocations have no consumer-defined return payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolbarResult {}

pub trait ToolbarProvider: Send + Sync {
    fn invoke(&self, ctx: &dyn ProviderInvocation) -> Result<ToolbarResult, ProviderError>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionValidation {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub credentials: std::collections::BTreeMap<String, String>,
}

impl ConnectionValidation {
    pub fn valid() -> Self {
        Self {
            valid: true,
            credentials: Default::default(),
        }
    }

    pub fn invalid() -> Self {
        Self {
            valid: false,
            credentials: Default::default(),
        }
    }

    pub fn credential(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.credentials.insert(name.into(), value.into());
        self
    }
}

pub trait ConnectionProvider: Send + Sync {
    fn validate(&self, ctx: &dyn ProviderInvocation) -> Result<ConnectionValidation, ProviderError> {
        Err(ProviderError::method_not_found(ctx.method()))
    }

    fn open(&self, ctx: &dyn ProviderInvocation) -> Result<ConnectionOpenResult, ProviderError> {
        Err(ProviderError::method_not_found(ctx.method()))
    }

    fn create(&self, ctx: &dyn ProviderInvocation) -> Result<ConnectionOpenResult, ProviderError> {
        Err(ProviderError::method_not_found(ctx.method()))
    }
}

fn unsupported(op: FsOperation) -> ProviderError {
    let mut error = ProviderError::method_not_found(op.methods()[0]);
    error.message = format!("Unsupported fs operation: {}", op.as_str());
    error
}
