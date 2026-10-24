# Connection content synchronization

Core owns on-demand materialization, persistent offline directory metadata, recursive pins, eviction, local change detection, retries and conflict presentation. Providers implement protocol I/O only. This applies to every connection type, without provider IDs hard-coded in Core.

Declare a connection factory under `contributions.connections`. No `capabilities` configuration is needed or accepted. Implement `fs.stat({connection,nodeId}) -> {revision,size?}` and `fs.writeFile({connection,nodeId,source,size,ifRevision}) -> {revision}`. Directory entries should include stable node IDs and revisions. A missing listing revision is obtained with stat at download time.

`source` is a logical filename. Only the exact Core-created upload snapshot is granted to this operation. Declare `permissions.files.read: true`; this remains subject to user approval. Open it with `ctx.files().open_read(source)`. SMB providers can copy bounded chunks to a server-side staging file and rename after validation.

For HTTP backends use `ctx.http().put(url).body_reader(reader).send()`. The SDK consumes the reader. Core verifies ownership of the read handle and sends the file directly through its HTTP client, so no entire-file buffers or absolute paths cross IPC. Inline `.body(...)` and `.json(...)` clear the file body. Request/task cleanup closes the original handle. Redirects remain subject to network allowlists; a redirect to GET drops the file body.

All providers implement the same `fs.writeFile` operation. Protocol-specific multipart/resumable upload stays inside the provider; Google Drive reads the exact Core snapshot in bounded 256 KiB chunks. Core does not select a transport mode from manifest metadata.

Revisions must change when remote content changes. Check `ifRevision` before writing. Use server-enforced conditions where available. Protocols with a precommit comparison can detect prior/concurrent changes before the final write but cannot promise a server-side compare-and-swap. There is no `sync`, `writeMode`, or `conflictControl` manifest switch. Do not silently claim atomic conflict guarantees on protocols that cannot provide them. Upload failures must leave Core's snapshot/local contents available for retry.

## Core-owned VFS views and connection eligibility

A standalone virtual tree declares `contributions.vfs: [{"id":"history","title":"Git history","readOnly":true}]`.
A connection factory declares `contributions.connections`; it implicitly supports internal VFS and must not repeat a scheme or VFS declaration.
UI contributions never confer provider eligibility. `scheme`, `schemes`, `uriPrefix`, `mountId`, and `mountPath` are forbidden in these contributions.

Within a selected-resource request, call `ctx.open_vfs_view("history", selected_root, immutable_context)`.
Core authenticates the process identity, contribution ownership, and resource scope, and returns a `fiex://mount/<id>/` navigation URI.
The plugin must not construct URIs. Core sends contributionId/viewContext/nodeId with vfs.list/stat/readSmall/search; return opaque nodeId plus Entry metadata, and relativePath for search. Root nodeId is empty.
Core binds returned entries to its mount and retains only explicitly saved small view descriptors across restarts.

Connection-specific fs.* provider requests are Core-only and require an installed, active connection factory bound to that instance.
Ordinary VFS registration does not grant sync, pin, eviction, conflict, or cache control. Generic files/HTTP/TCP remain subject to their existing operation/network grants.
These are application navigation URIs, not OS mounts. Connection files stream to Core-managed cache files before external opening.
Standalone readSmall exports are bounded (1 MiB complete file); larger exports fail explicitly pending a streaming VFS contract.
