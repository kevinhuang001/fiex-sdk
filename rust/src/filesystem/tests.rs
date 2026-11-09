use super::*;
use crate::{ProviderInvocationExt, RequestContext, RpcError};
use serde_json::{Value, json};

use crate::{
    context::ContextInner,
    runtime::{HostLink, session::Session},
    tasks::TaskManager,
};
use std::{
    io::Cursor,
    sync::{Arc, atomic::AtomicBool},
};

struct NoopLink;
impl HostLink for NoopLink {
    fn call(&self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Err(RpcError::method_not_found(method))
    }
    fn closed(&self) -> bool {
        false
    }
}

fn request(method: &str, params: Value) -> RequestContext {
    RequestContext::new(
        ContextInner::new(
            Arc::new(NoopLink),
            Arc::new(TaskManager::new(2)),
            Session::Operation(1),
            Arc::new(AtomicBool::new(false)),
        ),
        method.into(),
        params,
    )
}

struct Memory {
    writable: bool,
}

impl VfsProvider for Memory {
    fn operations(&self) -> &'static [crate::filesystem::FsOperation] {
        &[
            crate::filesystem::FsOperation::List,
            crate::filesystem::FsOperation::Stat,
            crate::filesystem::FsOperation::Read,
            crate::filesystem::FsOperation::Write,
        ]
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Ok(FsListing::new([
            FsEntry::directory("root", "/"),
            FsEntry::file("a", format!("a-{}", request.node_id().unwrap_or("-")))
                .with_size(4)
                .with_revision("r1"),
        ]))
    }

    fn stat(&self, _request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Ok(FsEntry::file("a", "a.txt").with_size(4).with_revision("r1"))
    }

    fn read(&self, _request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        Ok(FsRead::from_bytes(b"abcd".to_vec()))
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
        if !self.writable {
            return Err(RpcError::method_not_found("vfs.beginWrite"));
        }
        let revision = format!("r-{}", request.if_revision().unwrap_or("initial"));
        Ok(FsWrite::with_commit(Cursor::new(Vec::new()), move || {
            Ok(FsEntry::file("a", "a.txt").with_revision(revision))
        }))
    }
}

struct ReadOnly;

impl VfsProvider for ReadOnly {
    fn operations(&self) -> &'static [crate::filesystem::FsOperation] {
        &[crate::filesystem::FsOperation::List]
    }

    fn list(&self, _request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Ok(FsListing::default())
    }
}

#[test]
fn capabilities_follow_the_explicit_declaration() {
    let memory = Memory { writable: true };
    assert_eq!(
        memory.operations(),
        &[
            FsOperation::List,
            FsOperation::Stat,
            FsOperation::Read,
            FsOperation::Write
        ]
    );
    let readonly = ReadOnly;
    assert_eq!(readonly.operations(), &[FsOperation::List]);
    assert_eq!(
        FsServer::capability(&readonly).unwrap(),
        json!({"vfs": {"discovery":true,"preflightOps":[],"operations": ["list"]}})
    );
}

#[test]
fn operation_registry_dispatches_only_registered_preflight_and_does_not_grant_write() {
    struct Reviewed;
    impl VfsProvider for Reviewed {
        fn operations(&self) -> &'static [FsOperation] {
            &[FsOperation::List]
        }
        fn preflight_handlers(&self) -> Result<PreflightHandlers, RpcError> {
            let mut handlers = PreflightHandlers::new();
            handlers.on_import(|_, _, review| {
                Ok(if review.source.size.is_none_or(|size| size <= 10) {
                    PreflightDecision::allow()
                } else {
                    PreflightDecision::deny("size policy")
                })
            })?;
            Ok(handlers)
        }
    }
    let server = FsServer::new();
    let params = json!({"phase":"execute","operation":"import","args":{
        "parentId":"root","name":"x","transferKind":"copy",
        "source":{"name":"x","kind":"file","size":11,"modified":null,"extension":null,"mime":null,"executableShortcut":null,"system":null}}});
    let reply = server
        .serve(&Reviewed, &request("vfs.preflight", params.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(reply["allowed"], false);
    let capability = FsServer::capability(&Reviewed).unwrap();
    assert_eq!(capability["vfs"]["preflightOps"], json!(["import"]));
    let unregistered = server
        .serve(
            &Reviewed,
            &request(
                "vfs.preflight",
                json!({"phase":"execute","operation":"rename","args":{"nodeId":"n","newName":"b"}}),
            ),
        )
        .unwrap_err();
    assert_eq!(unregistered.code, -32601);
    assert!(
        server
            .serve(
                &Reviewed,
                &request("vfs.beginWrite", json!({"parentId":"root","name":"x"}))
            )
            .is_err()
    );
    assert!(
        server
            .serve(&ReadOnly, &request("vfs.preflight", params))
            .is_err()
    );
    assert!(
        server
            .serve(&Reviewed, &request("vfs.preflight", json!({})))
            .is_err()
    );
}

#[test]
fn unimplemented_operations_are_refused_before_the_provider() {
    let server = FsServer::new();
    let provider = ReadOnly;
    let error = server
        .serve(&provider, &request("vfs.remove", json!({"nodeId": "x"})))
        .unwrap_err();
    assert_eq!(error.code, -32601);
    assert_eq!(error.data.unwrap()["operation"], "remove");
}

#[test]
fn trash_and_remove_dispatch_to_distinct_provider_methods() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Deletions {
        trashed: Arc<AtomicUsize>,
        removed: Arc<AtomicUsize>,
    }
    impl VfsProvider for Deletions {
        fn operations(&self) -> &'static [crate::filesystem::FsOperation] {
            &[
                crate::filesystem::FsOperation::Trash,
                crate::filesystem::FsOperation::Remove,
            ]
        }
        fn trash(&self, _: &FsRequest<'_>) -> Result<(), RpcError> {
            self.trashed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn remove(&self, _: &FsRequest<'_>) -> Result<(), RpcError> {
            self.removed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let trashed = Arc::new(AtomicUsize::new(0));
    let removed = Arc::new(AtomicUsize::new(0));
    let provider = Deletions {
        trashed: trashed.clone(),
        removed: removed.clone(),
    };
    let server = FsServer::new();
    server
        .serve(&provider, &request("vfs.trash", json!({"nodeId":"n"})))
        .unwrap();
    server
        .serve(&provider, &request("vfs.remove", json!({"nodeId":"n"})))
        .unwrap();
    assert_eq!(trashed.load(Ordering::SeqCst), 1);
    assert_eq!(removed.load(Ordering::SeqCst), 1);
}

struct AccountFs;
impl VfsProvider for AccountFs {
    fn operations(&self) -> &'static [FsOperation] {
        &[FsOperation::Stat, FsOperation::Read, FsOperation::Write]
    }
    fn discover(
        &self,
        ctx: &dyn fiex_provider_contracts::ProviderInvocation,
    ) -> Result<Vec<FsOperation>, RpcError> {
        let writable = match ctx.input()? {
            fiex_provider_contracts::ProviderInput::Vfs(request) => request
                .connection
                .as_ref()
                .and_then(|values| values.get("writable"))
                .is_some_and(|value| {
                    value == &fiex_provider_contracts::ConnectionFieldValue::Boolean(true)
                }),
            _ => false,
        };
        Ok(if writable {
            vec![
                FsOperation::Stat,
                FsOperation::Read,
                FsOperation::Write,
                FsOperation::Remove,
            ]
        } else {
            vec![FsOperation::Stat, FsOperation::Read]
        })
    }
    fn stat(&self, _: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Ok(FsEntry::file("a", "a"))
    }
    fn write(&self, _: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
        panic!("denied write reached provider")
    }
}
#[test]
fn discovery_is_connection_scoped_and_cannot_widen_the_ceiling() {
    let server = FsServer::new();
    let read_only = json!({"connection":{"writable":false},"nodeId":"a"});
    let writable = json!({"connection":{"writable":true},"nodeId":"a"});
    let ops = server
        .serve(&AccountFs, &request("vfs.discover", writable))
        .unwrap()
        .unwrap();
    assert_eq!(ops["operations"], json!(["stat", "read", "write"]));
    let entry = server
        .serve(&AccountFs, &request("vfs.stat", read_only.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(entry["allowedOps"], json!(["stat", "read"]));
    assert!(
        server
            .serve(&AccountFs, &request("vfs.beginWrite", read_only))
            .is_err()
    );
}

#[test]
fn non_fs_methods_fall_through() {
    let server = FsServer::new();
    assert!(
        server
            .serve(&ReadOnly, &request("custom.thing", json!({})))
            .unwrap()
            .is_none()
    );
}

mod streams;

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
