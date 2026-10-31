use super::*;
use crate::{RequestContext, RpcError};
use serde_json::{json, Value};

use super::*;
use crate::{
    context::ContextInner,
    runtime::{session::Session, HostLink},
    tasks::TaskManager,
};
use std::{
    io::Cursor,
    sync::{atomic::AtomicBool, Arc},
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
        FsServer::capability(&readonly),
        json!({"vfs": {"discovery":true,"preflight":false,"operations": ["list"]}})
    );
}

#[test]
fn transfer_preflight_is_metadata_only_and_does_not_grant_write() {
    struct Reviewed;
    impl VfsProvider for Reviewed {
        fn operations(&self) -> &'static [FsOperation] {
            &[FsOperation::List]
        }
        fn supports_preflight(&self) -> bool {
            true
        }
        fn preflight(
            &self,
            _ctx: &RequestContext,
            review: &TransferPreflight,
        ) -> Result<PreflightDecision, RpcError> {
            Ok(PreflightDecision {
                allowed: review.metadata.size.is_none_or(|size| size <= 10),
                reason: Some("size policy".into()),
            })
        }
    }
    let server = FsServer::new();
    let params = json!({"nodeId":"root","direction":"import","action":"copy","name":"x","sameMount":false,
        "metadata":{"name":"x","kind":"file","size":11,"modified":null,"extension":null,"mime":null,"executableShortcut":null,"system":false}});
    let reply = server
        .serve(&Reviewed, &request("vfs.preflight", params.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(reply["allowed"], false);
    assert!(server
        .serve(
            &Reviewed,
            &request("vfs.beginWrite", json!({"parentId":"root","name":"x"}))
        )
        .is_err());
    assert!(server
        .serve(&ReadOnly, &request("vfs.preflight", params))
        .is_err());
    assert!(server
        .serve(&Reviewed, &request("vfs.preflight", json!({})))
        .is_err());
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

struct AccountFs;
impl VfsProvider for AccountFs {
    fn operations(&self) -> &'static [FsOperation] {
        &[FsOperation::Stat, FsOperation::Read, FsOperation::Write]
    }
    fn discover(&self, ctx: &RequestContext) -> Result<Vec<FsOperation>, RpcError> {
        Ok(if ctx.params()["connection"]["writable"] == true {
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
    assert!(server
        .serve(&AccountFs, &request("vfs.beginWrite", read_only))
        .is_err());
}

#[test]
fn non_fs_methods_fall_through() {
    let server = FsServer::new();
    assert!(server
        .serve(&ReadOnly, &request("custom.thing", json!({})))
        .unwrap()
        .is_none());
}

mod streams;
