//! Typed VFS storage provider contract, operation DTOs and stream transport.
//! Core native backends and plugin IPC use the same operation vocabulary.
mod preflight;
pub use preflight::{
    PreflightDecision, TransferAction, TransferDirection, TransferMetadata, TransferPreflight,
};
pub const MAX_CHUNK: usize = 4 * 1024 * 1024;
pub use fiex_provider_contracts::FsOperation;

mod types;
pub use types::*;
mod streams;
pub use streams::*;
mod provider;
pub use provider::VfsProvider;
use provider::{discover, invoke, unsupported};
mod server;
pub use server::FsServer;
#[cfg(test)]
mod tests;
