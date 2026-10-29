//! Metadata-only policy hook. An allow decision never grants an operation.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferAction {
    Copy,
    Move,
    Export,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Import,
    Export,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferMetadata {
    pub name: String,
    pub kind: String,
    pub size: Option<u64>,
    pub modified: Option<u64>,
    pub extension: Option<String>,
    pub mime: Option<String>,
    /// None means unknown, never proof that a file is an executable shortcut.
    pub executable_shortcut: Option<bool>,
    pub system: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreflight {
    /// Import: destination directory. Export: source node. Provider-owned ID.
    pub node_id: String,
    pub direction: TransferDirection,
    pub action: TransferAction,
    /// The final destination basename, including keep-both renames.
    pub name: String,
    pub metadata: TransferMetadata,
    pub same_mount: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreflightDecision {
    pub allowed: bool,
    #[serde(default)]
    pub reason: Option<String>,
}
