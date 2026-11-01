# Protocol 6 filesystem contract

Feature plugins use typed `Files` with a `ResourceRef` to call Core's `fs.*` methods. The reference is an identity bound to the current operation scope, not a path or bearer credential. Core checks the manifest, current grant, scope, view and adapter before storage IO. Connection and standalone storage providers implement `VfsProvider` and receive the separate `vfs.*` methods. `FsOperation` is the shared operation vocabulary. `plugin.initialize` echoes `protocolVersion: 6.0` and reports `sdkVersion`; Core rejects a mismatched version. There is no `host.fs.*` fallback.

The public provider traits, provider errors, request projection, and VFS request/stream types are defined by `fiex-provider-contracts`. The SDK implements those traits for plugin providers and supplies local Host-service access through `ProviderInvocationExt`; the service handle is process-local and is never serialized to Core. Core native handlers must implement the same shared traits directly.

This is a breaking protocol change. Rebuild all plugins and update their manifests together with Core. The Core migration checkpoint is documented in `fiex/docs/architecture/unified-fs-progress.md`.
