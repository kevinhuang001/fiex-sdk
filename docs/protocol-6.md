# Protocol 6 filesystem contract

Feature plugins use typed `Files` with a `ResourceRef` to call Core's `fs.*` methods. The reference is an identity bound to the current operation scope, not a path or bearer credential. Core checks the manifest, current grant, scope, view and adapter before storage IO. Connection and standalone storage providers implement `VfsProvider` and receive the separate `vfs.*` methods. `FsOperation` is the shared operation vocabulary. `plugin.initialize` echoes `protocolVersion: 6.0` and reports `sdkVersion`; Core rejects a mismatched version. There is no `host.fs.*` fallback.

This is a breaking protocol change. Rebuild all plugins and update their manifests together with Core. The Core migration checkpoint is documented in `fiex/docs/architecture/unified-fs-progress.md`.
