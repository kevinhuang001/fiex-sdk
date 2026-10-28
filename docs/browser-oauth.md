# Browser OAuth

Plugins use `ctx.oauth().authorize(&OAuthRequest)` rather than opening browsers,
listening on ports, or inventing redirect URI schemes themselves. Core checks the
plugin's network permission and the exact authorization-endpoint hostname, binds a
random `127.0.0.1` port, opens the system browser, validates CSRF state and returns
`AuthorizationCode { code, redirect_uri, code_verifier }`. PKCE uses S256. The
listener closes after completion, denial, cancellation or timeout. Callback pages
never contain codes or tokens.

`host.oauth.authorize` takes `authorizationEndpoint`, `clientId`, `scopes`,
`parameters` and `timeoutMs` (default 300000, maximum 600000). The endpoint must be
HTTPS without userinfo, query or fragment. Additional parameters cannot override
Core's state, redirect, response type or PKCE fields. A session must remain active
for the duration of the request. The plugin's request/UI budget should exceed the
interactive timeout plus token-exchange time.

The plugin exchanges the code at its provider's token endpoint using host HTTP,
including the exact `redirect_uri` and `code_verifier`. Tokens remain in plugin and
Core memory; do not return them through a plugin UI RPC.

For connections, declare private generated field names in
`extensions.connections.credentials: ["refreshToken"]`. These are secret fields
that Core stores but the connection form does not ask the user to fill out.
`connection.validate` may return `ConnectionValidation::valid()
.credential("refreshToken", token).into_value()`. Core rejects undeclared secret
names, persists accepted values in the selected credential backend, and omits
secrets from editable config and frontend responses. Ordinary secret input fields
(such as a user-owned OAuth client secret) use the same backend.

Use optional `connections.description` and `connections.setupUrl` to explain how
to create an application. Setup URLs must be HTTPS. Google Drive requests a
user-created Desktop app client with Drive API enabled and opens browser consent
on first save; no shared client secret or manually copied refresh token is needed.
