# IPC coverage audit (protocol 4.0)

This audit compares the Core host and UI bridge surface with the public Rust
and UI SDKs. It is intentionally versioned with the SDK: add a row when Core
adds a method, and delete the row when a method is removed.

## Native host methods

| Core method                                                               | SDK surface                                                                     | Status                                                                                                                                              |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `host.fs.openRead`, `readOpen`, `readChunk`, `closeRead`                  | `Files::open_read` + `FileReader`                                               | Covered                                                                                                                                             |
| `host.fs.beginWrite`, `writeChunk`, `writeBuffer`, `commit`, `abort`      | `Files::begin_write` + `FileWriter`                                             | Covered                                                                                                                                             |
| `host.fs.list`, `stat`, `statBatch`, `child`, `mkdir`, `remove`, `rename` | `Files` methods                                                                 | Covered                                                                                                                                             |
| `host.fs.read`, `create`, `write`                                         | none                                                                            | Legacy one-shot forms. The SDK uses the streaming methods. Keep Core for old plugins, remove in the next major protocol after a deprecation window. |
| `host.fs.readOpen` shared buffers                                         | internal `runtime::transfer`                                                    | Not public; no plugin should manage buffers directly.                                                                                               |
| `host.net.http.open/read/close`                                           | `HttpClient` / `Response`                                                       | Covered                                                                                                                                             |
| `host.net.http`                                                           | none                                                                            | Legacy buffer-based form; superseded by streaming. Deprecate with `host.fs.read/create/write`.                                                      |
| `host.net.tcp.open/readBytes/writeBytes/shutdown/close`                   | `TcpClient`, `TcpStream`, `TcpReadHalf`, `TcpWriteHalf`, optional Tokio adapter | Covered                                                                                                                                             |
| `host.net.tcp.read/write`                                                 | none                                                                            | Legacy forms; superseded by `readBytes`/`writeBytes`. Deprecate.                                                                                    |
| `host.storage.*`                                                          | `Storage` (`get`, `set`, `delete`, `keys`, `compare_and_set`, `update`)         | Covered                                                                                                                                             |
| `host.secret.*`                                                           | `Secrets` (`get`, `set`, `delete`) with typed `SecretError`                     | Covered                                                                                                                                             |
| `host.terminal.ready/output/input/size/closed/end`                        | `ctx.terminal(session_id)`                                                      | Covered; SSH plugin migrated                                                                                                                        |
| `host.exec`, `host.process.run`                                           | `RequestContext::process_run`, `process_run_at`                                 | Covered                                                                                                                                             |
| `host.vfs.openView`                                                       | `RequestContext::open_vfs_view`                                                 | Covered                                                                                                                                             |
| `host.preview.create`                                                     | none                                                                            | No current plugin uses it. Candidate for removal unless a previewer needs Core-owned cache files.                                                   |
| `host.error.report`                                                       | `RequestContext::report_error`                                                  | Covered                                                                                                                                             |
| `host.buffer.*`                                                           | internal `ipc` module                                                           | Not public; low-level shared memory is an implementation detail.                                                                                    |

## Core system events

| Event                              | SDK surface                                            | Status                                                                                                                                                  |
| ---------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `info.resolve`                     | `InfoProvider` or manual dispatch                      | Typed provider added                                                                                                                                    |
| `preview.open`                     | `PreviewProvider` or manual dispatch                   | Typed provider added                                                                                                                                    |
| `decorations.resolve`              | `DecorationProvider` or manual dispatch                | Typed provider added                                                                                                                                    |
| `contextMenu.resolve/execute`      | `ContextMenuProvider` or manual dispatch               | Typed provider added                                                                                                                                    |
| `search.query`                     | `SearchProvider` + `SearchResults`, or manual dispatch | Typed provider added                                                                                                                                    |
| `hooks.invoke`                     | `HookProvider` or manual dispatch                      | Typed provider added                                                                                                                                    |
| `toolbar.invoke`                   | `ToolbarProvider` or manual dispatch                   | Typed provider added                                                                                                                                    |
| `connection.validate/open/create`  | `ConnectionProvider`, or manual dispatch               | Typed provider added                                                                                                                                    |
| `fs.list/stat/writeFile`           | `ConnectionProvider`, or manual dispatch               | Typed provider added                                                                                                                                    |
| `fs.openRead/readStream/closeRead` | none                                                   | Streaming lifecycle remains manual. A `ProviderReader` trait should own stream IDs, chunks and close on drop before this is wrapped.                    |
| `fs.readSmall`                     | none                                                   | Core does not use this for connection providers; keep only if a third-party provider still needs a bounded read helper. Candidate for protocol removal. |
| `vfs.list/stat/readSmall/search`   | `VfsProvider`, or manual dispatch                      | Typed provider added                                                                                                                                    |

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

## Deletion candidates

1. `fiex.vfs.*` was removed from the UI SDK (no host bridge accepts it).
2. `host.fs.read/create/write` and `host.net.tcp.read/write` should be removed
   from Core after a documented compatibility window; the SDK no longer emits
   them.
3. `host.net.http` (non-streaming) has the same status.
4. `host.preview.create` and connection `fs.readSmall` have no callers in Core
   or official plugins; remove them in the next protocol major unless a real
   use case appears.
5. `host.buffer.*` stays private. It is not an SDK contract.

Legacy methods must not be deleted in place while released plugins may call
them. Core should keep a deprecation test and remove them only together with a
protocol major.
