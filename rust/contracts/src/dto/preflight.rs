//! Operation-specific, transport-independent VFS preflight contracts.
//!
//! A positive decision is an additional policy approval only. Core continues
//! to enforce the caller's scope and effective storage capabilities.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreflightOp {
    CreateFile,
    CreateFolder,
    Rename,
    Remove,
    Import,
    Export,
    Move,
}

impl PreflightOp {
    pub const ALL: [Self; 7] = [
        Self::CreateFile,
        Self::CreateFolder,
        Self::Rename,
        Self::Remove,
        Self::Import,
        Self::Export,
        Self::Move,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreateFile => "createFile",
            Self::CreateFolder => "createFolder",
            Self::Rename => "rename",
            Self::Remove => "remove",
            Self::Import => "import",
            Self::Export => "export",
            Self::Move => "move",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreflightPhase {
    Availability,
    Execute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferKind {
    Copy,
    Move,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoveMode {
    Trash,
    Permanent,
}

/// Portable facts about a transfer source. Unknown facts stay unknown; a
/// missing value must never be treated as proof that an item is trusted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferMetadata {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub modified: Option<u64>,
    #[serde(default)]
    pub extension: Option<String>,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub executable_shortcut: Option<bool>,
    #[serde(default)]
    pub system: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFilePreflight {
    pub parent_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderPreflight {
    pub parent_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreflight {
    pub node_id: String,
    #[serde(default)]
    pub new_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovePreflight {
    pub node_ids: Vec<String>,
    pub mode: RemoveMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreflight {
    pub parent_id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub source: TransferMetadata,
    pub transfer_kind: TransferKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreflight {
    pub node_id: String,
    pub transfer_kind: TransferKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePreflight {
    pub node_id: String,
    pub target_parent_id: String,
    #[serde(default)]
    pub target_name: Option<String>,
}

/// One tagged operation request. The operation-specific payload prevents
/// callers and handlers from exchanging unrelated optional fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "operation", content = "args", rename_all = "camelCase")]
pub enum PreflightOperation {
    CreateFile(CreateFilePreflight),
    CreateFolder(CreateFolderPreflight),
    Rename(RenamePreflight),
    Remove(RemovePreflight),
    Import(ImportPreflight),
    Export(ExportPreflight),
    Move(MovePreflight),
}

impl PreflightOperation {
    pub const fn op(&self) -> PreflightOp {
        match self {
            Self::CreateFile(_) => PreflightOp::CreateFile,
            Self::CreateFolder(_) => PreflightOp::CreateFolder,
            Self::Rename(_) => PreflightOp::Rename,
            Self::Remove(_) => PreflightOp::Remove,
            Self::Import(_) => PreflightOp::Import,
            Self::Export(_) => PreflightOp::Export,
            Self::Move(_) => PreflightOp::Move,
        }
    }
}

/// Phase is carried beside the tagged operation: Availability is suitable for
/// menus and may omit a not-yet-entered name; Execute has actual user input.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightRequest {
    pub phase: PreflightPhase,
    #[serde(flatten)]
    pub operation: PreflightOperation,
}

impl PreflightRequest {
    pub const fn new(phase: PreflightPhase, operation: PreflightOperation) -> Self {
        Self { phase, operation }
    }

    pub const fn op(&self) -> PreflightOp {
        self.operation.op()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let (required_name, ids): (Option<&str>, Vec<&str>) = match &self.operation {
            PreflightOperation::CreateFile(args) => (args.name.as_deref(), vec![&args.parent_id]),
            PreflightOperation::CreateFolder(args) => (args.name.as_deref(), vec![&args.parent_id]),
            PreflightOperation::Rename(args) => (args.new_name.as_deref(), vec![&args.node_id]),
            PreflightOperation::Import(args) => {
                if args.source.name.is_empty() || args.source.kind.is_empty() {
                    return Err("import requires source metadata");
                }
                (args.name.as_deref(), vec![&args.parent_id])
            }
            PreflightOperation::Move(args) => (
                args.target_name.as_deref(),
                vec![&args.node_id, &args.target_parent_id],
            ),
            PreflightOperation::Remove(args) => {
                if args.node_ids.is_empty() || args.node_ids.iter().any(String::is_empty) {
                    return Err("remove requires one or more node IDs");
                }
                if args
                    .node_ids
                    .iter()
                    .enumerate()
                    .any(|(index, node_id)| args.node_ids[..index].contains(node_id))
                {
                    return Err("remove node IDs must be unique");
                }
                (None, args.node_ids.iter().map(String::as_str).collect())
            }
            PreflightOperation::Export(args) => (None, vec![&args.node_id]),
        };
        if ids.iter().any(|id| id.is_empty()) {
            return Err("preflight requires non-empty owner-scoped IDs");
        }
        if required_name.is_some_and(str::is_empty) {
            return Err("operation name cannot be empty");
        }
        if self.phase == PreflightPhase::Execute
            && matches!(
                &self.operation,
                PreflightOperation::CreateFile(_)
                    | PreflightOperation::CreateFolder(_)
                    | PreflightOperation::Rename(_)
                    | PreflightOperation::Import(_)
            )
            && required_name.is_none()
        {
            return Err("execute preflight requires the final operation name");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightDecision {
    pub allowed: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl PreflightDecision {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            reason: None,
            code: None,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
            code: Some("PermissionDenied".into()),
        }
    }

    pub fn deny_with_code(code: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
            code: Some(code.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn requests() -> Vec<PreflightRequest> {
        let metadata = TransferMetadata {
            name: "source.txt".into(),
            kind: "file".into(),
            size: Some(12),
            modified: None,
            extension: Some("txt".into()),
            mime: None,
            executable_shortcut: None,
            system: None,
        };
        vec![
            PreflightRequest::new(
                PreflightPhase::Availability,
                PreflightOperation::CreateFile(CreateFilePreflight {
                    parent_id: "parent".into(),
                    name: None,
                }),
            ),
            PreflightRequest::new(
                PreflightPhase::Execute,
                PreflightOperation::CreateFolder(CreateFolderPreflight {
                    parent_id: "parent".into(),
                    name: Some("folder".into()),
                }),
            ),
            PreflightRequest::new(
                PreflightPhase::Execute,
                PreflightOperation::Rename(RenamePreflight {
                    node_id: "node".into(),
                    new_name: Some("renamed".into()),
                }),
            ),
            PreflightRequest::new(
                PreflightPhase::Execute,
                PreflightOperation::Remove(RemovePreflight {
                    node_ids: vec!["node".into()],
                    mode: RemoveMode::Permanent,
                }),
            ),
            PreflightRequest::new(
                PreflightPhase::Execute,
                PreflightOperation::Import(ImportPreflight {
                    parent_id: "parent".into(),
                    name: Some("source.txt".into()),
                    source: metadata,
                    transfer_kind: TransferKind::Move,
                }),
            ),
            PreflightRequest::new(
                PreflightPhase::Execute,
                PreflightOperation::Export(ExportPreflight {
                    node_id: "node".into(),
                    transfer_kind: TransferKind::Copy,
                }),
            ),
            PreflightRequest::new(
                PreflightPhase::Execute,
                PreflightOperation::Move(MovePreflight {
                    node_id: "node".into(),
                    target_parent_id: "parent".into(),
                    target_name: Some("source.txt".into()),
                }),
            ),
        ]
    }

    #[test]
    fn all_seven_operations_round_trip_with_tagged_camel_case_wire_format() {
        for (request, expected_op) in requests().into_iter().zip(PreflightOp::ALL) {
            assert_eq!(request.op(), expected_op);
            let value = serde_json::to_value(&request).unwrap();
            assert_eq!(value["operation"], expected_op.as_str());
            assert!(value["args"].is_object());
            let decoded: PreflightRequest = serde_json::from_value(value).unwrap();
            assert_eq!(decoded, request);
            assert_eq!(PreflightOp::parse(expected_op.as_str()), Some(expected_op));
        }
    }

    #[test]
    fn execute_requires_actual_names_and_rename_has_no_destination() {
        let availability = PreflightRequest::new(
            PreflightPhase::Availability,
            PreflightOperation::Rename(RenamePreflight {
                node_id: "node".into(),
                new_name: None,
            }),
        );
        assert!(availability.validate().is_ok());
        let execute = PreflightRequest::new(
            PreflightPhase::Execute,
            PreflightOperation::Rename(RenamePreflight {
                node_id: "node".into(),
                new_name: None,
            }),
        );
        assert!(execute.validate().is_err());
        let value = serde_json::to_value(requests()[2].clone()).unwrap();
        assert_eq!(value["operation"], "rename");
        assert!(value["args"].get("destination").is_none());
    }

    #[test]
    fn old_boolean_shape_and_malformed_operation_arguments_are_rejected() {
        let old: Result<PreflightRequest, _> = serde_json::from_value(json!({
            "nodeId":"n", "direction":"import", "action":"copy",
            "name":"a", "metadata":{"name":"a","kind":"file","system":false},
            "sameMount":false
        }));
        assert!(old.is_err());
        let missing_args: Result<PreflightRequest, _> = serde_json::from_value(json!({
            "phase":"execute", "operation":"rename", "args":{"nodeId":"n"}
        }));
        assert!(missing_args.is_ok());
        assert!(missing_args.unwrap().validate().is_err());
        let unknown: Result<Value, _> = serde_json::from_value(json!({"operation":"copy"}));
        assert!(unknown.is_ok());
        let unknown_op: Result<PreflightRequest, _> = serde_json::from_value(json!({
            "phase":"execute", "operation":"copy", "args":{}
        }));
        assert!(unknown_op.is_err());
    }
}
