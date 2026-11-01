//! Re-exports of the canonical provider contracts. The SDK defines no parallel
//! provider traits or response DTOs here.
use crate::{RequestContext, RpcError};

pub use fiex_provider_contracts::{
    ConnectionContextMenuProvider, ConnectionOpenResult, ConnectionProvider, ConnectionValidation,
    ContextMenuProvider, DecorationContribution, DecorationProvider, HookProvider, HookResult,
    InfoProvider, MenuItem, PreviewProvider, SearchEntry, SearchProvider, SearchResults, SearchTag,
    ToolbarProvider, ToolbarResult,
};

pub(super) fn unsupported(ctx: &RequestContext) -> RpcError {
    RpcError::method_not_found(ctx.method())
}
