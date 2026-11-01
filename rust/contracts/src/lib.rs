//! Shared, runtime-independent provider contracts for both Fiex Core
//! and resident plugin SDK implementations. No IPC, Tauri, HostLink or
//! platform dependencies belong in this crate.
pub mod dto;
pub use dto::{
    connection::*, connection_requests::*, decoration::*, errors::*, event_requests::*,
    matching::*, menu::*, pagination::*, preflight::*, presentations::*, provider::*,
    request_context::*, requests::*, resources::*, results::*, search::*, search_requests::*,
    vfs::*, vfs_request::*,
};

mod matcher;
pub use matcher::ProviderMatcher;

mod endpoint;
pub use endpoint::{event, Dispatch, Event};
mod error;
mod invocation;
pub use invocation::ProviderInvocation;
mod fs_request;
pub use fs_request::{FsProduct, FsRead, FsReadAt, FsRequest, FsWrite};
mod providers;
pub use providers::{
    ConnectionContextMenuProvider, ConnectionProvider, ContextMenuProvider,
    DecorationProvider, HookProvider, InfoProvider, PreviewProvider, SearchProvider,
    ToolbarProvider, VfsProvider,
};
mod instances;
pub use instances::ProviderInstances;
