use super::*;

#[test]
fn sdk_vfs_listing_round_trips_through_the_shared_core_wire_contract() {
    let listing = FsListing {
        entries: vec![FsEntry::file("opaque", "sample.txt")],
        next_cursor: Some("cursor-2".into()),
        warnings: vec!["partial metadata".into()],
        truncated: true,
    };
    let json = serde_json::to_value(listing).unwrap();
    let page: fiex_provider_contracts::VfsPage = serde_json::from_value(json).unwrap();
    assert!(page.validate_page(1).is_ok());
    assert_eq!(page.entries[0]["nodeId"], "opaque");
    assert_eq!(page.next_cursor.as_deref(), Some("cursor-2"));
    assert!(page.truncated);
}
