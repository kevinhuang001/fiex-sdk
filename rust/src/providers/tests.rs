use super::*;
use crate::{InfoRow, InfoSection};

struct ExampleInfo;
impl InfoProvider for ExampleInfo {
    fn resolve(&self, _ctx: &RequestContext) -> Result<Vec<InfoSection>, RpcError> {
        Ok(vec![InfoSection {
            source: String::new(),
            provider: String::new(),
            plugin_id: None,
            id: "example".into(),
            title: "Example".into(),
            rows: vec![InfoRow {
                label: "A".into(),
                value: "B".into(),
                copyable: false,
            }],
        }])
    }
}

#[test]
fn search_results_omit_false_truncation() {
    let value = serde_json::to_value(SearchResults::default()).unwrap();
    assert_eq!(value, json!({"entries":[]}));
}

#[test]
fn providers_are_composable() {
    let providers = Providers::new().info(ExampleInfo).search(ExampleInfoSearch);
    assert!(providers.info.len() == 1);
    assert!(providers.search.len() == 1);
    assert!(providers.preview.is_empty());
}

#[test]
fn multiple_named_instances_require_an_explicit_selection() {
    let providers = Providers::new()
        .info_named("metadata", ExampleInfo)
        .info_named("media", ExampleInfo);
    assert_eq!(providers.info.len(), 2);
    assert!(providers.info.select(&json!({})).is_err());
    assert!(providers
        .info
        .select(&json!({"providerId":"media"}))
        .unwrap()
        .is_some());
    assert!(providers
        .info
        .select(&json!({"providerId":"other"}))
        .is_err());
}

#[test]
fn duplicate_registration_does_not_overwrite_existing_provider() {
    let providers = Providers::new()
        .info_named("metadata", ExampleInfo)
        .info_named("metadata", ExampleInfo);
    assert_eq!(providers.info.len(), 1);
    assert_eq!(providers.registration_errors.len(), 1);
}

struct FixtureFs {
    operations: &'static [crate::filesystem::FsOperation],
}
impl crate::filesystem::VfsProvider for FixtureFs {
    fn operations(&self) -> &'static [crate::filesystem::FsOperation] {
        self.operations
    }
}

#[test]
fn named_vfs_providers_are_independently_selected() {
    use crate::filesystem::FsOperation;
    let providers = Providers::new()
        .filesystem_named(
            "drive",
            FixtureFs {
                operations: &[FsOperation::List],
            },
        )
        .filesystem_named(
            "git",
            FixtureFs {
                operations: &[FsOperation::Stat],
            },
        );
    assert_eq!(providers.filesystem.len(), 2);
    assert!(providers.filesystem.select(&json!({})).is_err());
    assert!(providers
        .filesystem
        .select(&json!({"providerId":"drive"}))
        .unwrap()
        .is_some());
    assert!(providers
        .filesystem
        .select(&json!({"providerId":"missing"}))
        .is_err());
    let capabilities = providers.capabilities();
    assert_eq!(capabilities["vfs"]["operations"], json!(["list", "stat"]));
    assert_eq!(capabilities["vfs"]["discovery"], true);
}

#[test]
fn duplicate_named_vfs_does_not_replace_existing_stream_registry() {
    use crate::filesystem::FsOperation;
    let providers = Providers::new()
        .filesystem_named(
            "drive",
            FixtureFs {
                operations: &[FsOperation::Read],
            },
        )
        .filesystem_named(
            "drive",
            FixtureFs {
                operations: &[FsOperation::Write],
            },
        );
    assert_eq!(providers.filesystem.len(), 1);
    assert_eq!(providers.registration_errors.len(), 1);
    assert_eq!(
        providers.capabilities()["vfs"]["operations"],
        json!(["read"])
    );
}

struct ExampleInfoSearch;
impl SearchProvider for ExampleInfoSearch {
    fn query(&self, _ctx: &RequestContext) -> Result<SearchResults, RpcError> {
        Ok(SearchResults::default())
    }
}
