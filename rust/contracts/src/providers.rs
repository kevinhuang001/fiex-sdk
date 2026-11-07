//! Canonical typed provider traits. SDK runtime and Core native handlers
//! implement these same interfaces; transport adapters stay outside this crate.
use crate::{
    ConnectionOpenResult, ConnectionValidation, CreateFilePreflight, CreateFolderPreflight,
    DecorationContribution, ExportPreflight, FsEntry, FsListing, FsOperation, FsRequest,
    HookResult, ImportPreflight, InfoFieldKey, InfoFieldResult, InfoSectionDescriptor,
    MenuExecuteResult, MenuItem, MovePreflight, PreflightDecision, PreflightOp, PreflightOperation,
    PreflightPhase, PreflightRequest, PreviewView, ProviderError, ProviderInvocation,
    RemovePreflight, RenamePreflight, SearchCapabilities, SearchEntry, SearchMode, SearchResults,
    ToolbarResult, TrashPreflight,
};
use std::{collections::BTreeMap, sync::Arc};

type PreflightHandler = dyn Fn(&dyn ProviderInvocation, &PreflightRequest) -> Result<PreflightDecision, ProviderError>
    + Send
    + Sync;

/// One source of truth for a provider's operation-specific policy handlers.
/// The SDK derives the negotiated `preflightOps` set from this registry.
#[derive(Clone, Default)]
pub struct PreflightHandlers {
    handlers: BTreeMap<PreflightOp, Arc<PreflightHandler>>,
}

impl PreflightHandlers {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(
        &mut self,
        op: PreflightOp,
        handler: Arc<PreflightHandler>,
    ) -> Result<(), ProviderError> {
        if self.handlers.contains_key(&op) {
            return Err(ProviderError::invalid_params(format!(
                "duplicate preflight handler: {}",
                op.as_str()
            )));
        }
        self.handlers.insert(op, handler);
        Ok(())
    }

    pub fn on_create_file<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &CreateFilePreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::CreateFile,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::CreateFile(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn on_create_folder<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &CreateFolderPreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::CreateFolder,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::CreateFolder(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn on_rename<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &RenamePreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::Rename,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::Rename(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn on_remove<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &RemovePreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::Remove,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::Remove(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn on_trash<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &TrashPreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::Trash,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::Trash(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn on_import<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &ImportPreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::Import,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::Import(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn on_export<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &ExportPreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::Export,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::Export(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn on_move<F>(&mut self, handler: F) -> Result<(), ProviderError>
    where
        F: Fn(
                &dyn ProviderInvocation,
                PreflightPhase,
                &MovePreflight,
            ) -> Result<PreflightDecision, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        self.insert(
            PreflightOp::Move,
            Arc::new(move |ctx, request| match &request.operation {
                PreflightOperation::Move(args) => handler(ctx, request.phase, args),
                _ => Err(ProviderError::invalid_params(
                    "preflight operation mismatch",
                )),
            }),
        )
    }

    pub fn supports(&self, op: PreflightOp) -> bool {
        self.handlers.contains_key(&op)
    }

    pub fn operations(&self) -> Vec<PreflightOp> {
        self.handlers.keys().copied().collect()
    }

    pub fn dispatch(
        &self,
        ctx: &dyn ProviderInvocation,
        request: &PreflightRequest,
    ) -> Result<PreflightDecision, ProviderError> {
        let handler = self.handlers.get(&request.op()).ok_or_else(|| {
            ProviderError::method_not_found(&format!("vfs.preflight.{}", request.op().as_str()))
        })?;
        handler(ctx, request)
    }
}

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
    fn describe(
        &self,
        ctx: &dyn ProviderInvocation,
    ) -> Result<Vec<InfoSectionDescriptor>, ProviderError>;

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

    #[test]
    fn registered_operations_are_derived_and_duplicates_are_rejected() {
        let mut handlers = PreflightHandlers::new();
        handlers
            .on_rename(|_, _, _| Ok(PreflightDecision::allow()))
            .unwrap();
        assert_eq!(handlers.operations(), vec![PreflightOp::Rename]);
        assert!(!handlers.supports(PreflightOp::Remove));
        assert!(handlers
            .on_rename(|_, _, _| Ok(PreflightDecision::allow()))
            .is_err());
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
