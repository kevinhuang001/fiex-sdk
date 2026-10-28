# Fiex Plugin SDK

Independent SDKs for developing plugins for [Fiex](https://github.com/kevinhuang001/fiex).

## 📖 Start here / 从这里开始

**第一次开发 Rust 插件？** 从这份图文式步骤教程开始，不需要先理解 JSON-RPC：

➡️ **[Rust 插件 SDK：从零开始（中文新手教程）](rust/README.md)**

The Chinese walkthrough takes you from an empty Rust project to a working native plugin, real-process tests, a `manifest.json`, cross-platform `.fiexplugin` packaging and GitHub Release installation.

### Which SDK should I use? / 该选哪个 SDK？

| 目录                      | 用途                                                                                                     |
| ------------------------- | -------------------------------------------------------------------------------------------------------- |
| [`rust/`](rust/README.md) | Native 插件 / Native process SDK：Rust 命令、文件系统适配器、宿主托管存储与凭据，采用 stdio JSON-RPC 2.0 |
| [`ui/`](ui/src/index.ts)  | UI SDK：在沙箱 iframe 中开发插件面板，通过 MessageChannel 与宿主通信                                     |

You can build a native-only plugin, a UI panel, or use both. A native plugin runs as a separate executable and must not write logs to stdout (use stderr instead).

## Quick SDK checks / 检查 SDK

```sh
# Rust SDK
cargo fmt --manifest-path rust/Cargo.toml --check
cargo test --manifest-path rust/Cargo.toml
cargo build --manifest-path rust/Cargo.toml --example mock_plugin

# Test the actual mock plugin process (Python 3 required)
python rust/examples/check_plugin.py --binary rust/target/debug/examples/mock_plugin --plugin-id org.fiex.mock --method mock.echo

# Browser-side UI SDK
npm --prefix ui install
npm --prefix ui run build
npm --prefix ui test
```

The Rust SDK currently requires **Rust 1.90+** and speaks **protocol 5.0**. For full explanations of each command, see the [beginner tutorial](rust/README.md).

## Real plugin examples / 真正的插件项目

See [fiex-official-plugins](https://github.com/kevinhuang001/fiex-official-plugins) for examples of Checksum, Archive, Git, SMB, and WebDAV plugins, including GitHub Actions packaging.

The APIs are **experimental** and may evolve with the Fiex Core development branch. Fiex does not bundle any official plugins or exempt official plugins from permission approval. Native plugins run inside Core's OS sandbox (Landlock/seccomp on Linux, Seatbelt on macOS, AppContainer and a Job Object on Windows); approved external commands run outside it as the current user. Only install plugins you trust.

Pin the SDK with a Git submodule when developing a plugin so that your SDK version matches the version tested with your host and plugin project.

## Protocol 5.0: refs, typed services and providers

Protocol 5.0 unifies every provider file access behind `fs.*` and removes host
paths from the public plugin API. `ResourceRef` is an
opaque `{mountId, nodeId}` identity; Core resolves it only inside the operation
scope. `ctx.targets()` contains the selected resources and their optional
permission-projected metadata. Never construct a ref or derive one from a path.

```rust,ignore
let target = ctx.target().ok_or_else(|| RpcError::invalid_params("missing target"))?;
let text = ctx.files().read_to_string(&target.reference)?;
```

`ctx.files().child(&directory.reference, "output.zip")` reserves a new identity
below an authorized directory. `ctx.storage()` is a typed, plugin-private store
with compare-and-set (`get`, `set`, `delete`, `keys`, `update`). `ctx.secrets()`
uses the system keychain and distinguishes `Denied`, `Unavailable`, missing and
invalid data instead of collapsing them into `None`. Terminal interfaces belong to PluginView packages. Plugins own
their session state and expose custom RPC through the UI bridge; the host only
brokers generic network and permission services.

Optional typed event providers live in `contracts`: `InfoProvider`,
`PreviewProvider`, `DecorationProvider`, `ContextMenuProvider`,
`SearchProvider`, `HookProvider`, `ToolbarProvider` and `ConnectionProvider`
(lifecycle only). File-system access is one `FileSystemProvider` trait for
connections and standalone views alike. Register only what the plugin
implements; custom RPC still goes through the fallback:

```rust,ignore
sdk::Providers::new()
    .info(MyInfo)
    .context_menu(MyMenu)
    .filesystem(MyDrive::new())
    .serve_with("org.example.plugin", |ctx| custom(ctx))
```

## File-system providers

Implement the methods you really support and annotate the impl block:

```rust,ignore
use fiex_plugin_sdk as sdk;
use sdk::filesystem::{FileSystemProvider, FsEntry, FsListing, FsRead, FsWrite};

#[sdk::filesystem]
impl FileSystemProvider for MyDrive {
    fn list(&self, request: &sdk::filesystem::FsRequest<'_>) -> Result<FsListing, sdk::RpcError> {
        // request.node_id(), request.cursor(), request.limit()
        Ok(FsListing::default())
    }

    fn read(&self, request: &sdk::filesystem::FsRequest<'_>) -> Result<FsRead, sdk::RpcError> {
        Ok(FsRead::new(http_response, Some(size)))
    }

    fn write(&self, request: &sdk::filesystem::FsRequest<'_>) -> Result<FsWrite, sdk::RpcError> {
        Ok(FsWrite::with_commit(upload_body, move || {
            Ok(FsEntry::file("id", "name").with_revision("etag"))
        }))
    }
}
```

The attribute macro records exactly the methods written in the block as the
plugin's capability set and generates the `fs.*` dispatcher. Methods inherited
from the trait's `unsupported` default are never advertised, so Core will not
offer `rename` or `remove` to a read-only provider. `plugin.initialize`
automatically replies with `{"fs":{"operations":[...]}}`; Core stores the
declaration and rejects undeclared operations before they reach the plugin.
Nodes can narrow the set further with `FsEntry::allowing([..])`.

The SDK owns stream ids, chunk offsets, `closeRead`, sequential write offsets
and `commit`/`abort`; providers only return standard `Read`/`Write` values.
Protocol-specific finishing (SMB staging rename, Drive resumable completion)
belongs in the commit callback.

## Streaming HTTP request bodies

Providers that upload bytes they generate themselves (WebDAV `PUT`, OpenList
uploads) can stream a request body instead of buffering the whole file:

```rust,ignore
let mut body = ctx.http().put(url).header("If-Match", etag).body_stream(size)?;
for chunk in chunks {
    body.write_chunk(chunk)?;
}
let response = body.finish()?.error_for_status()?;
```

`body_stream` sends `host.net.http.open {bodyStream: true, contentLength}` and
forwards every `write_chunk` through `host.net.http.body`; Core feeds a bounded
channel into the request, so memory stays at one chunk. Streamed requests do not
follow redirects because the body cannot be replayed. `body_reader` still
exists for Core-authorized `ResourceRef` snapshots.

## UI SDK wrappers

`FiexUiClient` now has typed helpers for the host bridge:
`fiex.clipboard.writeText(text)`, `fiex.navigation.open(path)`,
`fiex.preview.ready()`, `fiex.plugin.task(taskId)`, `fiex.plugin.invoke(...)`,
`fiex.dialog.*`, `fiex.tasks.*`, `fiex.panel.*`, `fiex.theme.*` and
`fiex.i18n.*`. The old `fiex.vfs.*` helper was removed: no Core panel bridge
exposes those methods, and plugin VFS access belongs in the native backend.

## Standard Info/Preview events

`info.resolve` and `preview.open` receive the same
`contracts::EventContext` with a `targets` array. The JSON-RPC `method` is the
system event identifier; no separate `intent`, `target`, or `selection` is
required. Plugins use `ctx.targets()` to read the operation targets and
`ctx.metadata(&target.reference)` for permission-projected metadata, without
performing IO. To access file content, use the scoped file SDK; a target ref
alone never grants read permission. Info returns `sections JSON`;
Preview returns `contracts::PreviewView` (for example,
`PreviewView::html("ui/preview.html")`). Custom plugin RPC methods such as
`checksum.calculate` remain available via the controlled plugin UI bridge.

## Core-owned VFS views and connection eligibility

A standalone virtual tree declares `extensions.vfs: [{"id":"history","title":"Git history","readOnly":true}]`.
A connection factory declares `extensions.connections` and additionally declares `extensions.vfs` when it exposes files; providers must not declare a scheme or construct mount identifiers.
UI extensions never confer provider eligibility. `scheme`, `schemes`, `uriPrefix`, `mountId`, and `mountPath` are forbidden in these extensions.

Within a selected-resource request, call `ctx.open_vfs_view("history", selected_root, immutable_context)`.
Core authenticates the process identity, contribution ownership, and resource scope, and returns a `fiex://mount/<id>/` navigation URI.
The plugin must not construct URIs. Core sends contributionId/viewContext/mountId/nodeId with the same `fs.*` methods a connection uses; return opaque nodeId plus entry metadata, and relativePath for search. Root nodeId is empty.
Core binds returned entries to its mount and retains only explicitly saved small view descriptors across restarts.

Connection-specific fs.* provider requests are Core-only and require an installed, active connection factory bound to that instance.
Ordinary VFS registration does not grant sync, pin, eviction, conflict, or cache control. Generic files/HTTP/TCP remain subject to their existing operation/network grants.
These are application navigation URIs, not OS mounts. Connection files stream to Core-managed cache files before external opening.
Standalone views stream through the same `fs.openRead/readStream/closeRead` contract, so exports and previews are no longer limited to 1 MiB.

## TCP split and half-close

`let (reader, writer) = ctx.tcp().connect(host, port)?.into_split();`
returns separately owned `TcpReadHalf: Read + Send` and
`TcpWriteHalf: Write + Send`. The host has independent TCP read/write locks,
so the two halves can be moved to separate worker threads. Call
`writer.shutdown()` for a TCP FIN while continuing to read a server response
through `reader`. The blocking API is always available. Enable the optional `tokio` feature for
`ctx.tcp().connect_async(...)` and `stream.into_async()`; the adapter preserves
completed reads across cancellation and submits owned writes through the same
broker. `ctx.tcp().with_deadline(instant)` applies one absolute deadline to
connect, reads and writes.
