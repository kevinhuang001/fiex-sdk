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

struct ExampleInfoSearch;
impl SearchProvider for ExampleInfoSearch {
    fn query(&self, _ctx: &RequestContext) -> Result<SearchResults, RpcError> {
        Ok(SearchResults::default())
    }
}
