//! Core event method routing to a selected, typed SDK provider instance.
use super::*;
use fiex_provider_contracts::ProviderInvocation;
impl Providers {
    pub(super) fn dispatch(&self, ctx: &RequestContext) -> Result<Option<Value>, RpcError> {
        let input = ctx.input()?;
        if ctx.method().starts_with("vfs.") {
            let fs = self
                .filesystem
                .select(input.provider_id())?
                .ok_or_else(|| unsupported(ctx))?;
            return fs.server.serve_with_preflight(
                fs.dispatch.as_ref(),
                &fs.preflight_handlers,
                ctx,
            );
        }
        let value = match ctx.method() {
            "info.compute" => {
                let provider = self
                    .info
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?;
                let fiex_provider_contracts::ProviderInput::InfoCompute(request) = input else {
                    return Err(RpcError::invalid_params("missing info compute request"));
                };
                let fields = provider.compute(ctx, &request.fields)?;
                json!({"fields":fields})
            }
            "preview.open" => {
                let view = self
                    .preview
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .open(ctx)?;
                serde_json::to_value(view).map_err(|error| RpcError::internal(error.to_string()))?
            }
            "decorations.resolve" => {
                let decorations = self
                    .decorations
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"decorations":decorations})
            }
            "contextMenu.resolve" => {
                let items = self
                    .context_menu
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"items":items})
            }
            "contextMenu.execute" => serde_json::to_value(
                self.context_menu
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .execute(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            "connectionContextMenu.resolve" => {
                let items = self
                    .connection_context_menu
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"items":items})
            }
            "connectionContextMenu.execute" => serde_json::to_value(
                self.connection_context_menu
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .execute(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            "search.query" => {
                let provider = self
                    .search
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?;
                let mode = match input {
                    fiex_provider_contracts::ProviderInput::Search(request) => request.mode,
                    _ => return Err(RpcError::invalid_params("missing search request")),
                };
                if !provider.supports(mode) {
                    return Err(unsupported(ctx));
                }
                serde_json::to_value(provider.query(ctx)?)
                    .map_err(|error| RpcError::internal(error.to_string()))?
            }
            "hooks.invoke" => serde_json::to_value(
                self.hooks
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .invoke(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            "toolbar.invoke" => serde_json::to_value(
                self.toolbar
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .invoke(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            "connection.validate" => serde_json::to_value(
                self.connection
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .validate(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            "connection.open" => serde_json::to_value(
                self.connection
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .open(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            "connection.create" => serde_json::to_value(
                self.connection
                    .select(input.provider_id())?
                    .ok_or_else(|| unsupported(ctx))?
                    .create(ctx)?,
            )
            .map_err(|error| RpcError::internal(error.to_string()))?,
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}
