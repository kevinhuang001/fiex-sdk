# Transfer metadata preflight

Implement `VfsProvider::supports_preflight() -> true` to opt into `vfs.preflight`.
Override `preflight(&RequestContext, &TransferPreflight)` and return
`PreflightDecision { allowed, reason }`. `FsServer` handles the handshake,
request parsing and response serialization. Missing or invalid request fields
are errors. The hook does not change `operations`, `discover` or node `allowedOps`.

- `direction`: `import` reviews the destination directory; `export` reviews the
  source node. `nodeId` always belongs to the receiving provider.
- `action`: `copy`, `move` or external `export`.
- `name`: final destination basename, including keep-both collision renames.
- `metadata`: source name, kind, optional size/modified/extension/MIME,
  optional executable-shortcut classification, and system-entry flag.
- `sameMount`: whether the source and destination have the same Core mount identity.

A provider can check size, name, kind, quota or server policy. Unknown metadata
is represented by `None`; the provider chooses whether unknown values are allowed.
The hook must be read-only and repeatable: Core may review the same transfer
at planning, execution and cleanup. An allow reply is advisory, not a reservation
or authorization token; enforce server permissions again during mutation.
No host paths, file contents or peer credentials are sent for metadata review.
