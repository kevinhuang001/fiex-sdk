use super::*;
use crate::{FsOperation, ResourceCapabilities, ResourceMetadata};
use serde_json::json;

fn location(resource_type: crate::ResourceType, provider: &str, mount: &str) -> ResourceLocation {
    ResourceLocation {
        resource_type,
        provider_id: provider.into(),
        mount_id: Some(mount.into()),
    }
}

fn target(
    kind: &str,
    view: Option<ResourceLocation>,
    storage: Option<ResourceLocation>,
) -> MatchResource {
    MatchResource {
        metadata: Some(ResourceMetadata {
            kind: kind.into(),
            extension: Some("zip".into()),
            mime: Some("application/zip".into()),
            ..Default::default()
        }),
        view,
        storage,
        capabilities: ResourceCapabilities {
            operations: vec![FsOperation::Read],
        },
    }
}

#[test]
fn location_matches_are_conjunctive_and_must_be_present() {
    let rule: ProviderMatchRule = serde_json::from_value(json!({
        "view":{"providerIds":["core:recent"]},
        "storage":{"types":["vfs"],"mountIds":["account-a"]},
        "kinds":["file"],"requiredOps":["read"]
    }))
    .unwrap();
    let entry = target(
        "file",
        Some(location(
            crate::ResourceType::Vfs,
            "core:recent",
            "core:recent",
        )),
        Some(location(
            crate::ResourceType::Vfs,
            "plugin:drive",
            "account-a",
        )),
    );
    assert!(rule.matches_targets(std::slice::from_ref(&entry), 1));
    assert!(!rule.matches_targets(&[target("file", entry.view.clone(), None)], 1));
    assert!(!rule.matches_targets(&[entry.clone(), MatchResource::default()], 2));
}

#[test]
fn typed_matcher_checks_content_kind_and_effective_capabilities() {
    let rule: ProviderMatchRule = serde_json::from_value(json!({
        "extensions":["*.ZIP"],
        "mimeTypes":["application/*"],
        "kinds":["file"],
        "requiredOps":["read"]
    }))
    .unwrap();
    let file = target("file", None, None);
    assert!(rule.matches_targets(std::slice::from_ref(&file), 1));
    let mut directory = file.clone();
    directory.metadata.as_mut().unwrap().kind = "directory".into();
    assert!(!rule.matches_targets(&[directory], 1));
    let mut unreadable = file;
    unreadable.capabilities.operations.clear();
    assert!(!rule.matches_targets(&[unreadable], 1));
}

#[test]
fn connections_use_the_same_view_and_storage_constraints() {
    let rule: ProviderMatchRule = serde_json::from_value(json!({
        "kinds":["connection"],
        "view":{"types":["vfs"],"providerIds":["plugin:drive"]},
        "storage":{"mountIds":["connection-a"]}
    }))
    .unwrap();
    let connection = target(
        "connection",
        Some(location(
            crate::ResourceType::Vfs,
            "plugin:drive",
            "connection-a",
        )),
        Some(location(
            crate::ResourceType::Vfs,
            "plugin:drive",
            "connection-a",
        )),
    );
    assert!(rule.matches_targets(std::slice::from_ref(&connection), 1));
    assert!(!rule.matches_targets(
        &[target(
            "file",
            connection.view.clone(),
            connection.storage.clone()
        )],
        1
    ));
    assert!(!rule.matches_targets(&[target("connection", connection.view.clone(), None)], 1));
}

#[test]
fn shared_matcher_uses_trusted_directory_for_zero_selection() {
    let rule: ProviderMatchRule = serde_json::from_value(json!({
        "view":{"providerIds":["core:applications"]},
        "selection":"zero",
        "kinds":["directory"]
    }))
    .unwrap();
    let directory_location = location(
        crate::ResourceType::Vfs,
        "core:applications",
        "core:applications",
    );
    let mut directory = target(
        "directory",
        Some(directory_location.clone()),
        Some(directory_location),
    );
    directory.metadata.as_mut().unwrap().extension = None;
    directory.metadata.as_mut().unwrap().mime = None;
    directory.capabilities.operations = vec![FsOperation::List, FsOperation::Mkdir];
    assert!(ProviderMatcher::matches(&rule, &[], Some(&directory), 0));
    assert!(!ProviderMatcher::matches(&rule, &[], None, 0));
    assert!(!ProviderMatcher::matches(&rule, &[], Some(&directory), 1));
}

#[test]
fn optional_location_fields_serialize_without_null_selector_values() {
    let rule = ProviderMatchRule {
        view: Some(LocationMatch {
            provider_ids: Some(vec!["core:applications".into()]),
            ..Default::default()
        }),
        ..Default::default()
    };
    let value = serde_json::to_value(&rule).unwrap();
    assert_eq!(value["view"]["providerIds"][0], "core:applications");
    assert!(value["view"].get("types").is_none());
    assert!(value["view"].get("mountIds").is_none());
    assert!(rule.validate_consistency().is_ok());
}

#[test]
fn rejects_invalid_resource_types_and_empty_selectors() {
    for value in [
        json!({"view":{"types":["remote"]}}),
        json!({"view":{"types":[]}}),
        json!({"view":{"providerIds":[]}}),
        json!({"kinds":["vfs"]}),
        json!({"storage":{"domains":["drive"]}}),
    ] {
        assert!(
            ProviderMatchRule::validate(Some(&value), true).is_err(),
            "{value}"
        );
    }
}

#[test]
fn exclusions_match_any_selected_resource_and_and_across_dimensions() {
    let rule: ProviderMatchRule = serde_json::from_value(json!({
        "exclude": {
            "view": {"providerIds": ["core:applications", "core:trash"]},
            "requiredOps": ["read"]
        }
    }))
    .unwrap();
    let local = target("file", None, None);
    let applications = target(
        "file",
        Some(location(
            crate::ResourceType::Vfs,
            "core:applications",
            "core:applications",
        )),
        None,
    );
    assert!(rule.matches_targets(std::slice::from_ref(&local), 1));
    assert!(!rule.matches_targets(&[local.clone(), applications.clone()], 2));

    let mut unreadable = applications;
    unreadable.capabilities.operations.clear();
    assert!(rule.matches_targets(&[local, unreadable], 2));
}

#[test]
fn exclusions_use_the_directory_for_blank_selection() {
    let excluded: ProviderMatchRule = serde_json::from_value(json!({
        "selection": "zero",
        "requiredOps": ["write"],
        "exclude": {"view": {"providerIds": ["core:applications"]}}
    }))
    .unwrap();
    let included: ProviderMatchRule = serde_json::from_value(json!({
        "selection": "zero",
        "requiredOps": ["mkdir"]
    }))
    .unwrap();
    let view = location(
        crate::ResourceType::Vfs,
        "core:applications",
        "core:applications",
    );
    let mut directory = target("directory", Some(view), None);
    directory.capabilities.operations = vec![FsOperation::Write, FsOperation::Mkdir];
    assert!(!ProviderMatcher::matches(
        &excluded,
        &[],
        Some(&directory),
        0
    ));
    assert!(ProviderMatcher::matches(
        &included,
        &[],
        Some(&directory),
        0
    ));
    assert!(!ProviderMatcher::matches(
        &included,
        &[],
        Some(&directory),
        1
    ));
}

#[test]
fn invalid_exclusions_are_rejected_including_empty_objects() {
    for value in [
        json!({"exclude": {}}),
        json!({"exclude": {"view": {"providerIds": []}}}),
        json!({"exclude": {"selection": "zero"}}),
        json!({"exclude": {"exclude": {"view": {"providerIds": ["core:trash"]}}}}),
        json!({"exclude": {"requiredOps": ["not-an-operation"]}}),
    ] {
        assert!(
            ProviderMatchRule::validate(Some(&value), true).is_err(),
            "{value}"
        );
    }
}
