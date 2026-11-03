# VFS operation preflight

Preflight is an optional, read-only policy check for storage-owned file
operations. The Core always checks the operation's effective capabilities and
scope first. A preflight `allow` adds no permission; it only means that the
provider has no additional dynamic restriction for this request.

## Register only the operations you own

Implement `VfsProvider::preflight_handlers` and add handlers for the operations
that need dynamic policy. The SDK derives `preflightOps` from this registry and
uses one `vfs.preflight` JSON-RPC method for all operations.

```rust
use fiex_plugin_sdk::filesystem::{
    PreflightDecision, PreflightHandlers, PreflightPhase, VfsProvider,
};
use fiex_plugin_sdk::RpcError;

fn preflight_handlers(&self) -> Result<PreflightHandlers, RpcError> {
    let mut handlers = PreflightHandlers::new();
    handlers.on_rename(|ctx, phase, args| {
        // Availability may not have a name yet. Execute has the final name.
        if phase == PreflightPhase::Execute && args.new_name.is_none() {
            return Err(RpcError::invalid_params("rename name is required"));
        }
        Ok(PreflightDecision::allow())
    })?;
    handlers.on_import(|ctx, _phase, args| {
        check_import_quota(ctx, &args.source)?;
        Ok(PreflightDecision::allow())
    })?;
    Ok(handlers)
}
```

If a provider has no handler for an operation, its negotiated list omits that
operation. Core skips the extra IPC check for it; basic capabilities and the
actual VFS method still govern whether the operation can run. Registering the
same operation twice is rejected.

## Operations and phases

`PreflightRequest` is a tagged operation with a typed `args` object. Its seven
operation names are `createFile`, `createFolder`, `rename`, `remove`, `import`,
`export`, and `move`. Create file and create folder are separate checks.
Rename carries `nodeId` and optional `newName`; it has no destination field.
Import carries only portable source metadata, never another provider's node ID,
host path, or credentials.

`Availability` checks whether an action may be offered. It can omit a name that
has not been entered yet. `Execute` includes the actual name or transfer target
and is checked again immediately before mutation. Both phases are read-only and
repeatable. Do not reserve resources or return an authorization token.

For create and import requests, the parent ID belongs to the receiving
provider. For rename, remove, export, and move, node IDs belong to the
provider handling the request. Move is used only within one mount; cross-mount
move is checked as Export plus Import, followed by a separate removal check.

## Decisions and failures

Return `PreflightDecision::allow()` or `PreflightDecision::deny(reason)`. A
denial is returned as a normal typed decision. A handler error, timeout,
disconnected plugin, malformed response, or invalid owner-scoped ID fails
closed. The Core reports execute failures through its Error Center.

Do not register a handler for ordinary OS ACL or server permission checks.
Those are enforced by the operating system or remote VFS operation. Do not use
Preflight for Copy/Cut clipboard recording, Read, List, Stat, Search, Preview,
Info, or Decoration.
