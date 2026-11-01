//! Re-exports of the canonical provider contracts. The SDK defines no parallel
//! provider traits or response DTOs here.
use crate::{RequestContext, RpcError};

pub use fiex_provider_contracts::{
    ConnectionContextMenuProvider, ConnectionProvider, ConnectionValidation, ContextMenuProvider,
    DecorationBadge, DecorationContribution, DecorationField, DecorationProvider,
    DecorationStyle, HookProvider, HookResult, InfoProvider, PreviewProvider, SearchProvider,
    SearchResults, ToolbarProvider, ToolbarResult,
};

pub(super) fn unsupported(ctx: &RequestContext) -> RpcError {
    RpcError::method_not_found(ctx.method())
}
