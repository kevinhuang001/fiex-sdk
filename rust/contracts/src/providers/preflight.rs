use crate::{
    CreateFilePreflight, CreateFolderPreflight, ExportPreflight, ImportPreflight, MovePreflight,
    PreflightDecision, PreflightOp, PreflightOperation, PreflightPhase, PreflightRequest,
    ProviderError, ProviderInvocation, RemovePreflight, RenamePreflight, TrashPreflight,
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
