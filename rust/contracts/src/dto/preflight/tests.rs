use super::*;
use serde_json::{Value, json};

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
            PreflightOperation::Trash(TrashPreflight {
                node_ids: vec!["node".into()],
            }),
        ),
        PreflightRequest::new(
            PreflightPhase::Execute,
            PreflightOperation::Remove(RemovePreflight {
                node_ids: vec!["node".into()],
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
fn all_eight_operations_round_trip_with_tagged_camel_case_wire_format() {
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

    let legacy_remove: Result<PreflightRequest, _> = serde_json::from_value(json!({
        "phase":"execute", "operation":"remove",
        "args":{"nodeIds":["n"], "mode":"trash"}
    }));
    assert!(legacy_remove.is_err());
}
