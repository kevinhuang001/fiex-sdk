//! Core event method routing to a selected, typed SDK provider instance.
use super::*;
impl Providers {
    pub(super) fn dispatch(&self, ctx: &RequestContext) -> Result<Option<Value>, RpcError> {
        if ctx.method().starts_with("vfs.") {
            let fs = self.filesystem.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?;
            return fs.server.serve(fs.dispatch.as_ref(), ctx);
        }
        let value = match ctx.method() {
            "info.resolve" => {
                let sections = self
                    .info
                    .select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"sections":sections})
            }
            "preview.open" => {
                let view = self
                    .preview
                    .select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?
                    .open(ctx)?;
                serde_json::to_value(view).map_err(|error| RpcError::internal(error.to_string()))?
            }
            "decorations.resolve" => {
                let decorations = self
                    .decorations
                    .select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"decorations":decorations})
            }
            "contextMenu.resolve" => {
                let items = self
                    .context_menu
                    .select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"items":items})
            }
            "contextMenu.execute" => self
                .context_menu
                .select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?
                .execute(ctx)?,
            "connectionContextMenu.resolve" => {
                let items = self
                    .connection_context_menu
                    .select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?
                    .resolve(ctx)?;
                json!({"items":items})
            }
            "connectionContextMenu.execute" => self
                .connection_context_menu
                .select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?
                .execute(ctx)?,
            "search.query" => {
                let provider = self
                    .search
                    .select(ctx.params())?
                    .ok_or_else(|| unsupported(ctx))?;
                let mode: crate::filesystem::SearchMode = serde_json::from_value(
                    ctx.params().get("mode").cloned().unwrap_or(json!("name")),
                )
                .map_err(|_| RpcError::invalid_params("invalid search mode"))?;
                if !provider.supports(mode) {
                    return Err(unsupported(ctx));
                }
                serde_json::to_value(provider.query(ctx)?)
                    .map_err(|error| RpcError::internal(error.to_string()))?
            }
            "hooks.invoke" => self
                .hooks
                .select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?
                .invoke(ctx)?,
            "toolbar.invoke" => self
                .toolbar
                .select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?
                .invoke(ctx)?,
            "connection.validate" => self.connection.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?
                .validate(ctx)?,
            "connection.open" => self.connection.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?
                .open(ctx)?,
            "connection.create" => self.connection.select(ctx.params())?
                .ok_or_else(|| unsupported(ctx))?
                .create(ctx)?,
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}
