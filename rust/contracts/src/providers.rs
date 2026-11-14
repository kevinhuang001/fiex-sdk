//! Canonical typed provider traits. SDK runtime and Core native handlers
//! implement these same interfaces; transport adapters stay outside this crate.
use crate::{
    ConnectionOpenResult, ConnectionValidation, DecorationContribution, FsEntry, FsListing,
    FsOperation, FsRequest, HookResult, InfoFieldKey, InfoFieldResult, MenuExecuteResult, MenuItem,
    PreviewView, ProviderError, ProviderInvocation, SearchCapabilities, SearchEntry, SearchMode,
    SearchResults, ToolbarResult,
};
mod preflight;
pub use preflight::PreflightHandlers;

pub trait VfsProvider: Send + Sync {
    /// Register only the dynamic policy checks this provider owns. The SDK
    /// calls this once when the provider is registered and derives support
    /// declarations from the returned handlers.
    fn preflight_handlers(&self) -> Result<PreflightHandlers, ProviderError> {
        Ok(PreflightHandlers::new())
    }

    fn operations(&self) -> &'static [FsOperation];

    fn search_capabilities(&self, _ctx: &dyn ProviderInvocation) -> SearchCapabilities {
        SearchCapabilities::default()
    }

    fn discover(&self, _ctx: &dyn ProviderInvocation) -> Result<Vec<FsOperation>, ProviderError> {
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

    fn trash(&self, request: &FsRequest<'_>) -> Result<(), ProviderError> {
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
    fn compute(
        &self,
        ctx: &dyn ProviderInvocation,
        fields: &[InfoFieldKey],
    ) -> Result<Vec<InfoFieldResult>, ProviderError>;
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

pub trait HookProvider: Send + Sync {
    fn invoke(&self, ctx: &dyn ProviderInvocation) -> Result<HookResult, ProviderError>;
}

pub trait ToolbarProvider: Send + Sync {
    fn invoke(&self, ctx: &dyn ProviderInvocation) -> Result<ToolbarResult, ProviderError>;
}

pub trait ConnectionProvider: Send + Sync {
    fn validate(
        &self,
        ctx: &dyn ProviderInvocation,
    ) -> Result<ConnectionValidation, ProviderError> {
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

#[cfg(test)]
mod preflight_registry_tests {
    use super::*;
    use crate::{PreflightDecision, PreflightOp};

    #[test]
    fn registered_operations_are_derived_and_duplicates_are_rejected() {
        let mut handlers = PreflightHandlers::new();
        handlers
            .on_rename(|_, _, _| Ok(PreflightDecision::allow()))
            .unwrap();
        assert_eq!(handlers.operations(), vec![PreflightOp::Rename]);
        assert!(!handlers.supports(PreflightOp::Remove));
        assert!(
            handlers
                .on_rename(|_, _, _| Ok(PreflightDecision::allow()))
                .is_err()
        );
    }

    #[test]
    fn trash_and_remove_preflight_handlers_are_independent() {
        let mut handlers = PreflightHandlers::new();
        handlers
            .on_trash(|_, _, _| Ok(PreflightDecision::deny("trash policy")))
            .unwrap();
        assert!(handlers.supports(PreflightOp::Trash));
        assert!(!handlers.supports(PreflightOp::Remove));
        handlers
            .on_remove(|_, _, _| Ok(PreflightDecision::allow()))
            .unwrap();
        assert_eq!(
            handlers.operations(),
            vec![PreflightOp::Trash, PreflightOp::Remove]
        );
    }
}
