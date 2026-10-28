# Fiex Rust SDK 0.3.0 — protocol 6

Feature providers use typed, ResourceRef-only `Files` calls over `fs.*`. Storage
providers implement `VfsProvider`, advertise `FsOperation` and narrow operations
through discovery; Core invokes them over `vfs.*`. Protocol mismatches fail closed.
No legacy host filesystem or host terminal API is retained.

Readers and writers use bounded IPC/shared windows, operation-owned handles,
seek and explicit commit/abort. Readers reject premature EOF rather than silently
returning a truncated checksum or archive. Content beyond the opened snapshot
length is not returned.

This release packages Rust SDK 0.3.0 with the unchanged browser UI SDK 0.1.1.
Release tags follow the Rust package version so the earlier UI tag cannot suppress
publication of a new native protocol. PluginViews own their UI and business RPC;
Core supplies generic file, network, process, OAuth, storage and secret services.
