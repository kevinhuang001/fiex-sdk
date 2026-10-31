use super::*;
use crate::{RequestContext, RpcError};

pub(super) fn unsupported(op: FsOperation) -> RpcError {
    let mut error = RpcError::method_not_found(op.methods()[0]);
    error.message = format!("Unsupported fs operation: {}", op.as_str());
    error
}

/// The trait a plugin implements for a connection or a standalone view.
///
/// Explicitly declare the implementation ceiling with `operations`.
/// `discover` narrows it for the current authenticated connection/resource;
/// unknown permissions should return an empty set, never optimistic writes.
pub trait VfsProvider: Send + Sync {
    /// Opt in to per-transfer metadata review. Capabilities remain independent.
    fn supports_preflight(&self) -> bool {
        false
    }

    fn preflight(
        &self,
        _ctx: &RequestContext,
        _request: &TransferPreflight,
    ) -> Result<PreflightDecision, RpcError> {
        Ok(PreflightDecision {
            allowed: true,
            reason: None,
        })
    }

    fn operations(&self) -> &'static [FsOperation];

    /// Advertise only modes whose native semantics are actually implemented.
    /// An operation ceiling alone never implies support for both modes.
    fn search_capabilities(&self, _ctx: &RequestContext) -> SearchCapabilities {
        SearchCapabilities::default()
    }

    /// Server permissions for this connection/node. SDK intersects this result
    /// with `operations`; discovery can never widen the implementation ceiling.
    fn discover(&self, _ctx: &RequestContext) -> Result<Vec<FsOperation>, RpcError> {
        Ok(self.operations().to_vec())
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Err(unsupported(request.op()))
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Err(unsupported(request.op()))
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        Err(unsupported(request.op()))
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
        Err(unsupported(request.op()))
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Err(unsupported(request.op()))
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Err(unsupported(request.op()))
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), RpcError> {
        Err(unsupported(request.op()))
    }

    fn search(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Err(unsupported(request.op()))
    }
}

pub(super) fn invoke(provider: &dyn VfsProvider, request: &FsRequest<'_>) -> Result<FsProduct, RpcError> {
    Ok(match request.op() {
        FsOperation::List => FsProduct::Listing(provider.list(request)?),
        FsOperation::Stat => FsProduct::Entry(provider.stat(request)?),
        FsOperation::Read => FsProduct::Read(provider.read(request)?),
        FsOperation::Write => FsProduct::Write(provider.write(request)?),
        FsOperation::Mkdir => FsProduct::Entry(provider.mkdir(request)?),
        FsOperation::Rename => FsProduct::Entry(provider.rename(request)?),
        FsOperation::Remove => {
            provider.remove(request)?;
            FsProduct::Done
        }
        FsOperation::Search => FsProduct::Listing(provider.search(request)?),
    })
}

pub(super) fn discover(
    provider: &dyn VfsProvider,
    ctx: &RequestContext,
) -> Result<Vec<FsOperation>, RpcError> {
    let reported = provider.discover(ctx)?;
    Ok(provider
        .operations()
        .iter()
        .copied()
        .filter(|op| reported.contains(op))
        .collect())
}
