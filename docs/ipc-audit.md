# IPC coverage audit (protocol 6.0)

This audit compares the Core host and UI bridge surface with the public Rust
and UI SDKs. It is intentionally versioned with the SDK: add a row when Core
adds a method, and delete the row when a method is removed.

## Native host methods

| Core method                                                          | SDK surface                                                                     | Status                                                                     |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `fs.openRead`, `readOpen`, `readChunk`, `closeRead`                  | `Files::open_read` + `FileReader`                                               | Covered                                                                    |
| `fs.beginWrite`, `writeChunk`, `writeBuffer`, `commit`, `abort`      | `Files::begin_write` + `FileWriter`                                             | Covered                                                                    |
| `fs.list`, `stat`, `statBatch`, `child`, `mkdir`, `remove`, `rename` | `Files` methods                                                                 | Covered                                                                    |
| `fs.read`, `create`, `write`                                         | none                                                                            | Removed in protocol 4. SDK uses the streaming methods.                     |
| `fs.readOpen` shared buffers                                         | internal `runtime::transfer`                                                    | Not public; no plugin should manage buffers directly.                      |
| `host.net.http.open/read/close`                                      | `HttpClient` / `Response`                                                       | Covered                                                                    |
| `host.net.http`                                                      | none                                                                            | Removed in protocol 4; superseded by the streaming HTTP methods.           |
| `host.net.tcp.open/readBytes/writeBytes/shutdown/close`              | `TcpClient`, `TcpStream`, `TcpReadHalf`, `TcpWriteHalf`, optional Tokio adapter | Covered                                                                    |
| `host.net.tcp.read/write`                                            | none                                                                            | Removed in protocol 4; superseded by `readBytes`/`writeBytes`.             |
| `host.storage.*`                                                     | `Storage` (`get`, `set`, `delete`, `keys`, `compare_and_set`, `update`)         | Covered                                                                    |
| `host.secret.*`                                                      | `Secrets` (`get`, `set`, `delete`) with typed `SecretError`                     | Covered                                                                    |
| `host.terminal.ready/output/input/size/closed/end`                   | `ctx.terminal(session_id)`                                                      | Covered; SSH plugin migrated                                               |
| `host.process.run`                                                   | `RequestContext::process_run`, `process_run_at`                                 | Covered. The old `host.exec` entry was removed.                            |
| `host.vfs.openView`                                                  | `RequestContext::open_vfs_view`                                                 | Covered                                                                    |
| `host.preview.create`                                                | none                                                                            | Removed: no caller remained and preview documents use package/event views. |
| `host.error.report`                                                  | `RequestContext::report_error`                                                  | Covered                                                                    |
| `host.buffer.*`                                                      | internal `ipc` module                                                           | Not public; low-level shared memory is an implementation detail.           |

## Core system events

| Event                                     | SDK surface                                                   | Status                                                                             |
| ----------------------------------------- | ------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `info.resolve`                            | `InfoProvider` or manual dispatch                             | Typed provider added                                                               |
| `preview.open`                            | `PreviewProvider` or manual dispatch                          | Typed provider added                                                               |
| `decorations.resolve`                     | `DecorationProvider` or manual dispatch                       | Typed provider added                                                               |
| `contextMenu.resolve/execute`             | `ContextMenuProvider` or manual dispatch                      | Typed provider added                                                               |
| `search.query`                            | `SearchProvider` + `SearchResults`, or manual dispatch        | Typed provider added                                                               |
| `hooks.invoke`                            | `HookProvider` or manual dispatch                             | Typed provider added                                                               |
| `toolbar.invoke`                          | `ToolbarProvider` or manual dispatch                          | Typed provider added                                                               |
| `connection.validate/open/create`         | `ConnectionProvider`, or manual dispatch                      | Typed provider added                                                               |
| `vfs.list/stat/search`                    | `VfsProvider::{list,stat,search}` + `VfsProvider::operations` | Covered: one trait for connections and views.                                      |
| `vfs.openRead/readStream/closeRead`       | `VfsProvider::read` + `FsServer`                              | Covered: SDK owns stream IDs, offsets and close.                                   |
| `vfs.beginWrite/writeStream/commit/abort` | `VfsProvider::write` + `FsWrite`                              | Covered: providers return a standard `Write`; the commit callback publishes.       |
| `vfs.mkdir/rename/remove`                 | `VfsProvider::{mkdir,rename,remove}`                          | Covered.                                                                           |
| `fs.writeFile`                            | none                                                          | Removed in protocol 6; the streaming write path replaces the upload snapshot pull. |
| Protocol-4 standalone view methods        | none                                                          | Replaced by the unified protocol-5 `vfs.*` methods and stream sessions.            |
| `plugin.initialize` capabilities          | `Providers::capabilities` / `serve_declaring`                 | Added: `{"vfs":{"operations":[..]}}`, derived from the impl block.                 |

`Providers` is optional; existing plugins continue to use `sdk::serve` with a
method match. `Providers::serve_with` delegates any unregistered method to the
plugin's custom handler.

## UI bridge methods

| Bridge method                                      | UI SDK                           | Status                                                                                                      |
| -------------------------------------------------- | -------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `context.get`                                      | `fiex.context.get()`             | Covered                                                                                                     |
| `clipboard.writeText`                              | `fiex.clipboard.writeText(text)` | Added                                                                                                       |
| `navigation.open`                                  | `fiex.navigation.open(path)`     | Added                                                                                                       |
| `preview.ready`                                    | `fiex.preview.ready()`           | Added                                                                                                       |
| `plugin.invoke`                                    | `fiex.plugin.invoke(...)`        | Covered                                                                                                     |
| `plugin.task`                                      | `fiex.plugin.task(taskId)`       | Added                                                                                                       |
| `panel.close`, `panel.resize`                      | `fiex.panel.*`                   | Covered                                                                                                     |
| `plugin.error`                                     | `fiex.errors.report`             | Covered                                                                                                     |
| `theme.get`                                        | `fiex.theme.get/subscribe`       | Covered                                                                                                     |
| `dialog.openFile/openFiles/openDirectory/saveFile` | `fiex.dialog.*`                  | Covered                                                                                                     |
| `tasks.list/create/spinner/update/finish/cancel`   | `fiex.tasks.*`                   | Covered                                                                                                     |
| `locale.changed` event                             | `fiex.i18n.subscribe`            | Covered                                                                                                     |
| `vfs.stat/list/open`                               | none                             | Removed from the UI SDK. No Core panel bridge allows these methods; VFS work belongs in the native backend. |

## Removed in protocol 4

1. UI `fiex.vfs.*` was removed; no host bridge accepted it.
2. `fs.read/create/write` were removed; the SDK only emits streaming
   `openRead/readOpen/readChunk/closeRead` and `beginWrite/writeChunk/commit`.
3. `host.net.http` (non-streaming) was removed; the SDK emits
   `http.open/read/close`.
4. `host.net.tcp.read/write` were removed; the SDK emits
   `readBytes/writeBytes`.
5. `host.exec` was removed; the SDK emits `host.process.run`.
6. `host.preview.create` was removed; no caller remained.
7. The protocol-4 standalone-view contract was removed. Connections and views
   now share the protocol-5 `vfs.*` contract, with contribution identity and streaming reads.
8. `fs.writeFile` was removed; all writes use
   `vfs.beginWrite/writeStream/commit/abort`.

`host.buffer.*` stays private. It is not an SDK contract.

## Canonical VFS namespace

The current native file-provider methods and initialization capabilities use
`vfs.*` and `vfs: {operations, discovery}`. The former protocol-5 `fs.*` spellings
are not aliases. Update Core, SDK and providers together. Host filesystem
services remain `fs.*`; browser panel file services are unchanged.
