//! Core event method routing to a selected, typed SDK provider instance.
use super::*;
impl Providers {
    pub(super) fn dispatch(&self, ctx: &RequestContext) -> Result<Option<Value>, RpcError> {
        if ctx.method().starts_with("vfs.") {
            let Some(fs) = self.filesystem.select(ctx.params())? else {
                return Ok(None);
            };
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
            "connection.validate" => match self.connection.select(ctx.params())? {
                Some(provider) => provider.validate(ctx)?,
                None => return Ok(None),
            },
            "connection.open" => match self.connection.select(ctx.params())? {
                Some(provider) => provider.open(ctx)?,
                None => return Ok(None),
            },
            "connection.create" => match self.connection.select(ctx.params())? {
                Some(provider) => provider.create(ctx)?,
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}
