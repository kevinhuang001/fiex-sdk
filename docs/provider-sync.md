# Connection content synchronization

Core owns on-demand materialization, persistent offline directory metadata, recursive pins, eviction, local change detection, retries and conflict presentation. Providers implement protocol I/O only. This applies to every connection type, without provider IDs hard-coded in Core.

Declare a connection factory under `extensions.connections` and register an ordinary `VfsProvider`. `operations()` declares its implementation ceiling and `discover(ctx)` narrows it for the current account/resource. Core reads the `vfs: {operations, discovery}` initialization block.

Connections and standalone views use one native wire namespace: `vfs.list`, `vfs.stat`, `vfs.search`, `vfs.mkdir`, `vfs.rename`, `vfs.remove`, `vfs.discover` and the streaming methods below. `fs.*` is a separate Core file-service API used by `ctx.files()`; its names are unchanged.

Reads open with `vfs.openRead`, pull bounded chunks through `vfs.readStream`, and finish with `vfs.closeRead`. Writes begin with `vfs.beginWrite`, send ordered chunks with `vfs.writeStream`, and finish with `vfs.commit` or `vfs.abort`. A new file names `parentId` and `name`; replacement names `nodeId` and `ifRevision`. The SDK owns stream IDs, offsets and session authorization. Providers implement `Read`/`Write` behavior rather than receiving a Core upload-snapshot resource or an absolute path.

Protocol-specific staging, multipart and resumable HTTP upload remain inside the provider. For HTTP backends, `ctx.http().put(url).body_reader(reader).send()` streams a reader through Core's HTTP broker. No entire-file buffers or absolute paths cross IPC. Redirects remain subject to network allowlists.

Directory entries include stable node IDs and revisions. A missing listing revision is obtained with `vfs.stat` at download time.

Revisions must change when remote content changes. Check `ifRevision` before writing. Use server-enforced conditions where available. Protocols with a precommit comparison can detect prior/concurrent changes before the final write but cannot promise a server-side compare-and-swap. There is no `sync`, `writeMode`, or `conflictControl` manifest switch. Do not silently claim atomic conflict guarantees on protocols that cannot provide them. Upload failures must leave Core's snapshot/local contents available for retry.

## Core-owned VFS views and connection eligibility

A standalone virtual tree declares `extensions.vfs: {"title":"Git history","readOnly":true}`.
A connection factory declares `extensions.connections`; a connection that navigates to files must additionally declare `extensions.vfs`. Core owns the mount ID and URI for each connection.
UI extensions never confer provider eligibility. `scheme`, `schemes`, `uriPrefix`, `mountId`, and `mountPath` are forbidden in these extensions.

Within a selected-resource request, call `ctx.open_vfs_view("history", selected_root, immutable_context)`.
Core authenticates the process identity, contribution ownership, and resource scope, and returns a `fiex://mount/<id>/` navigation URI.
The plugin must not construct URIs. Core sends contributionId/viewContext/nodeId with `vfs.list/stat/openRead/readStream/closeRead/search`; return opaque nodeId plus Entry metadata, and relativePath for search. Root nodeId is empty.
Core binds returned entries to its mount and retains only explicitly saved small view descriptors across restarts.

Connection-specific vfs.* provider requests are Core-only and require an installed, active connection factory bound to that instance.
Ordinary VFS registration does not grant sync, pin, eviction, conflict, or cache control. Generic files/HTTP/TCP remain subject to their existing operation/network grants.
These are application navigation URIs, not OS mounts. Connection files stream to Core-managed cache files before external opening.
Standalone exports use the same bounded streaming reads as connections, without the old small-file export ceiling.
