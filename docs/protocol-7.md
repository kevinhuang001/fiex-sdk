# Protocol 7.0 provider contract

Feature plugins use typed `Files` with a `ResourceRef` to call Core's
`host.fs.*` service methods. The reference is an identity bound to the current
operation scope, not a path or bearer credential. Core checks the manifest,
current grant, scope, view and adapter before storage IO. Connection and
standalone storage providers implement `VfsProvider` and receive the separate
`vfs.*` methods. `FsOperation` is the shared operation vocabulary.
`plugin.initialize` echoes `protocolVersion: 7.0` and reports `sdkVersion`;
Core rejects a mismatched version. The former naked `fs.*` host-service names
are rejected.

The public provider traits, provider errors, request projection and VFS
request/stream types are defined by `fiex-provider-contracts`. The SDK
implements those traits for plugin providers and supplies local Host-service
access through `ProviderInvocationExt`; the service handle is process-local and
is never serialized to Core. Core native handlers implement the same shared
traits directly.

This is a breaking protocol change. Rebuild all plugins and update their
manifests together with Core. Protocol 6's filesystem rules remain in the
[Protocol 6 filesystem contract](protocol-6.md).

## Info field sessions

Protocol 7.0 replaces the eager `InfoProvider::resolve` response with two
calls. `info.describe` declares section and field descriptors (`id`, `label`,
and `compute: onOpen | onDemand`) without computing values. `info.compute`
receives only the `InfoFieldKey` values selected by Core and returns text
results for those keys. Info field caching, permissions, cancellation and
session ownership remain Core responsibilities. Protocol 7.0 does not accept
the former Info DTOs or RPC shape.
