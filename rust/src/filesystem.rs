//! Typed VFS storage provider contract, operation DTOs and stream transport.
//! Core native backends and plugin IPC use the same operation vocabulary.
pub use fiex_provider_contracts::{
    CreateFilePreflight, CreateFolderPreflight, ExportPreflight, ImportPreflight, MovePreflight,
    PreflightDecision, PreflightHandlers, PreflightOp, PreflightOperation, PreflightPhase,
    PreflightRequest, RemovePreflight, RenamePreflight, TransferKind, TrashPreflight,
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
