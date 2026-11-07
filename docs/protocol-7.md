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

Info fields are declared statically in `extensions.info.sections` in the
plugin manifest. Each field contains only `id`, `label`, and
`compute: onOpen | onDemand`; the Core builds the field index when the plugin
is installed or activated. Opening Info never starts a plugin just to discover
its fields. The SDK exposes only `InfoProvider::compute`; Core invokes
`info.compute` for the exact field keys selected by its scheduler or by a user
action. The plugin returns text results for those keys. Info field caching,
permissions, cancellation and session ownership remain Core responsibilities.
Protocol 7.0 does not accept the former Info DTOs or RPC shape.
