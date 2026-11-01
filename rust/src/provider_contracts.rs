//! Re-export of the shared event and request contract.
//! This module must not define its own runtime-specific routing semantics.
pub use fiex_provider_contracts::{event, Dispatch, Event, ProviderRequest};
