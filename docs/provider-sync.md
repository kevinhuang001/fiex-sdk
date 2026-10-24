# Connection content synchronization

Core owns on-demand materialization, persistent offline directory metadata, recursive pins, eviction, local change detection, retries and conflict presentation. Providers implement protocol I/O only. This applies to every connection type, without provider IDs hard-coded in Core.

Declare `capabilities: {"sync": true, "writeMode": "file"}` on a connection factory. Implement `fs.stat({connection,nodeId}) -> {revision,size?}` and `fs.writeFile({connection,nodeId,source,size,ifRevision}) -> {revision}`. Directory entries should include stable node IDs and revisions. A missing listing revision is obtained with stat at download time.

`source` is a logical filename. Only the exact Core-created upload snapshot is granted to this operation. Declare `permissions.files.read: true`; this remains subject to user approval. Open it with `ctx.files().open_read(source)`. SMB providers can copy bounded chunks to a server-side staging file and rename after validation.

For HTTP backends use `ctx.http().put(url).body_reader(reader).send()`. The SDK consumes the reader. Core verifies ownership of the read handle and sends the file directly through its HTTP client, so no entire-file buffers or absolute paths cross IPC. Inline `.body(...)` and `.json(...)` clear the file body. Request/task cleanup closes the original handle. Redirects remain subject to network allowlists; a redirect to GET drops the file body.

Providers with native multipart uploads may retain `writeMode: "stream"` (the default) and implement `fs.beginWrite`, `fs.writeStream`, `fs.commitWrite`, `fs.abortWrite`. Google Drive uses this mode. Both modes run through the same Core cache and reconciliation engine.

Revisions must change when remote content changes. Check `ifRevision` before writing. Use server-enforced conditions where available. Expose `conflictControl` as `conditional` or `compare-before-commit`: the latter detects prior/concurrent changes before the final write but cannot promise a server-side compare-and-swap. Do not silently claim atomic conflict guarantees on protocols that cannot provide them. Upload failures must leave Core's snapshot/local contents available for retry.
