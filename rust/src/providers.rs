//! Optional typed event providers.
//!
//! A plugin can keep its existing `sdk::serve` dispatcher, or register only the
//! system events it implements here. Custom RPC methods still go through the
//! fallback handler. Provider implementations stay optional and composable;
//! nothing forces a plugin to implement events it does not own.
use crate::{
    RequestContext, RpcError,
    filesystem::{FsServer, PreflightHandlers, VfsProvider},
};
use serde_json::{Value, json};

mod contracts;
use contracts::unsupported;
pub use contracts::{
    ConnectionContextMenuProvider, ConnectionProvider, ConnectionValidation, ContextMenuProvider,
    DecorationProvider, HookProvider, InfoProvider, MenuItem, PreviewProvider, SearchEntry,
    SearchProvider, SearchResults, SearchTag, ToolbarProvider, ToolbarResult,
};

mod dispatch;
mod registration;
mod registry;
mod serve;
use registry::ProviderSet;

/// Trait-specific SDK registry. One plugin can expose multiple distinct
/// providers of each kind, with stable IDs selected by trusted Core routing.
/// An omitted providerId is accepted only when exactly one instance exists.
struct FsProvider {
    dispatch: Box<dyn VfsProvider>,
    server: FsServer,
    preflight_handlers: PreflightHandlers,
}
pub struct Providers {
    diagnostics: bool,
    registration_errors: Vec<String>,
    filesystem: ProviderSet<FsProvider>,
    info: ProviderSet<dyn InfoProvider>,
    preview: ProviderSet<dyn PreviewProvider>,
    decorations: ProviderSet<dyn DecorationProvider>,
    context_menu: ProviderSet<dyn ContextMenuProvider>,
    connection_context_menu: ProviderSet<dyn ConnectionContextMenuProvider>,
    search: ProviderSet<dyn SearchProvider>,
    hooks: ProviderSet<dyn HookProvider>,
    toolbar: ProviderSet<dyn ToolbarProvider>,
    connection: ProviderSet<dyn ConnectionProvider>,
}

impl Default for Providers {
    fn default() -> Self {
        use fiex_provider_contracts::ProviderKind;
        Self {
            diagnostics: false,
            registration_errors: Vec::new(),
            filesystem: ProviderSet::new(ProviderKind::Vfs),
            info: ProviderSet::new(ProviderKind::Info),
            preview: ProviderSet::new(ProviderKind::Preview),
            decorations: ProviderSet::new(ProviderKind::Decoration),
            context_menu: ProviderSet::new(ProviderKind::ContextMenu),
            connection_context_menu: ProviderSet::new(ProviderKind::ConnectionContextMenu),
            search: ProviderSet::new(ProviderKind::Search),
            hooks: ProviderSet::new(ProviderKind::Hook),
            toolbar: ProviderSet::new(ProviderKind::Toolbar),
            connection: ProviderSet::new(ProviderKind::Connection),
        }
    }
}

#[cfg(test)]
mod tests;
