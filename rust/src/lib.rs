//! Fiex native plugin IPC SDK. The public wire format is independent of Rust.
//! Control channel: Content-Length framed JSON-RPC 2.0 over stdin/stdout.
//! Never use stdout for logging; stderr is available for diagnostics.
mod host;
mod serve;
mod types;

pub mod files;
pub mod http;
pub mod ipc;
pub mod tcp;
pub mod worker;

pub use host::{HostClient, HostServices};
pub use serve::{serve, serve_with_host};
pub use types::{read_frame, write_frame, RpcError, MAX_FRAME, PROTOCOL_VERSION};
