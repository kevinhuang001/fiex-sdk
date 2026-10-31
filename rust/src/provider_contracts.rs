//! Re-export of the one transport-neutral ProviderEndpoint/Event contract.
//! This module must not define its own runtime-specific routing semantics.
pub use fiex_provider_contracts::{event, Dispatch, Event, ProviderEndpoint, ProviderRequest};
