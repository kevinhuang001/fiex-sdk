//! Serializable provider-contract data shared by Core and the plugin SDK.
//!
//! Runtime traits, transport streams, and routing metadata live beside the
//! contract crate root. Wire request, result, resource, and capability DTOs
//! are kept here so consumers have one place to audit the public data model.

pub mod connection;
pub mod connection_requests;
pub mod decoration;
pub mod errors;
pub mod event_requests;
pub mod matching;
pub mod menu;
pub mod pagination;
pub mod preflight;
pub mod presentations;
pub mod provider;
pub mod request_context;
pub mod requests;
pub mod resources;
pub mod results;
pub mod search;
pub mod search_requests;
pub mod vfs;
pub mod vfs_request;

pub use connection::*;
pub use connection_requests::*;
pub use decoration::*;
pub use errors::ProviderError;
pub use event_requests::*;
pub use matching::*;
pub use menu::*;
pub use pagination::*;
pub use preflight::*;
pub use presentations::*;
pub use provider::*;
pub use request_context::ProviderRequestContext;
pub use requests::*;
pub use resources::*;
pub use results::{ConnectionValidation, HookResult, ToolbarResult};
pub use search::*;
pub use search_requests::*;
pub use vfs::*;
pub use vfs_request::*;
