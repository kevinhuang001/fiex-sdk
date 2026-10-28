# VfsProvider discovery contract

`VfsProvider` is an ordinary object-safe trait; there is no filesystem procedural
macro. Implement the required `operations() -> &'static [FsOperation]` to declare the
implementation ceiling. Register with `Providers::filesystem(provider)`.

The SDK announces `vfs: { operations: [...], discovery: true }` at initialization.
`discover(&RequestContext) -> Result<Vec<FsOperation>, RpcError>` reports the currently
permitted subset for the authenticated connection and resource. Its default is
the implementation ceiling. Remote providers override it using server permissions.
The SDK intersects the two sets; a discovery reply cannot add an unimplemented
operation. Errors fail closed, and an empty set permits no file operations.

Core sends `vfs.discover` with the same mount, connection/view identity and target
parameters as the operation. The reply is `{operations: ["list", "stat", ...]}`.
This is a negotiated protocol-5 extension: Core retains the static ceiling for
older providers that do not advertise `discovery: true`.

The SDK checks discovery before dispatch, including read/write stream opening,
and narrows returned node metadata (`allowedOps`). `allowedOps: []` is an explicit
denial, not an omitted restriction. Existing streams retain their close/abort
lifecycle; discovery does not orphan handles. Stream IDs, offsets and host handle
session authorization remain SDK-owned.

Discovery is advisory for presentation and is also checked at dispatch. User grants,
Core operation scopes, and the server's authorization of the actual request remain
independent checks. Providers must not infer account write access merely from a
supported HTTP method, successful login, or a write method present in their code.

Official adapters use SMB MAXIMUM_ALLOWED plus FILE_ACCESS_INFORMATION, WebDAV
DAV:current-user-privilege-set (including containing-collection bind/unbind), and
OpenList account bits intersected with the directory metadata write gate. Servers
that cannot describe write permissions conservatively expose reads; no mutation
is attempted just to probe a permission.
