//! Typed input union and codec for standard provider events.
use crate::{
    ConnectionMenuExecuteRequest, ConnectionMenuResolveRequest, ConnectionRequest,
    ContextMenuExecuteRequest, ContextMenuResolveRequest, HookRequest, InfoComputeRequest,
    ResourceEventRequest, SearchQueryRequest, ToolbarRequest, VfsRequest,
};
use serde_json::{Map, Value};

/// A decoded standard event. `Unspecified` is reserved for custom RPC methods
/// and stream transport methods, which have separate typed boundaries.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ProviderInput {
    #[default]
    Unspecified,
    Resource(ResourceEventRequest),
    InfoCompute(InfoComputeRequest),
    ContextMenuResolve(ContextMenuResolveRequest),
    ContextMenuExecute(ContextMenuExecuteRequest),
    ConnectionMenuResolve(ConnectionMenuResolveRequest),
    ConnectionMenuExecute(ConnectionMenuExecuteRequest),
    Search(SearchQueryRequest),
    Vfs(VfsRequest),
    Connection(ConnectionRequest),
    Hook(HookRequest),
    Toolbar(ToolbarRequest),
}

impl ProviderInput {
    pub fn decode(method: &str, params: &Value) -> Result<Self, String> {
        if !matches!(
            method,
            "info.compute"
                | "preview.open"
                | "decorations.resolve"
                | "contextMenu.resolve"
                | "contextMenu.execute"
                | "connectionContextMenu.resolve"
                | "connectionContextMenu.execute"
                | "search.query"
                | "vfs.discover"
                | "vfs.list"
                | "vfs.stat"
                | "vfs.openRead"
                | "vfs.beginWrite"
                | "vfs.mkdir"
                | "vfs.rename"
                | "vfs.trash"
                | "vfs.remove"
                | "vfs.search"
                | "connection.validate"
                | "connection.open"
                | "connection.create"
                | "hooks.invoke"
                | "toolbar.invoke"
        ) {
            return Ok(Self::Unspecified);
        }
        let mut object = params
            .as_object()
            .cloned()
            .ok_or_else(|| "provider event parameters must be an object".to_string())?;
        for key in [
            "targets",
            "directory",
            "selectionCount",
            "sessionId",
            "locale",
        ] {
            object.remove(key);
        }
        let projected = Value::Object(object);
        macro_rules! parse {
            ($ty:ty, $variant:ident) => {
                serde_json::from_value::<$ty>(projected.clone())
                    .map(ProviderInput::$variant)
                    .map_err(|error| format!("invalid {method} request: {error}"))
            };
        }
        match method {
            "preview.open" | "decorations.resolve" => {
                parse!(ResourceEventRequest, Resource)
            }
            "info.compute" => parse!(InfoComputeRequest, InfoCompute),
            "contextMenu.resolve" => parse!(ContextMenuResolveRequest, ContextMenuResolve),
            "contextMenu.execute" => parse!(ContextMenuExecuteRequest, ContextMenuExecute),
            "connectionContextMenu.resolve" => {
                parse!(ConnectionMenuResolveRequest, ConnectionMenuResolve)
            }
            "connectionContextMenu.execute" => {
                parse!(ConnectionMenuExecuteRequest, ConnectionMenuExecute)
            }
            "search.query" => parse!(SearchQueryRequest, Search),
            "vfs.discover" | "vfs.list" | "vfs.stat" | "vfs.openRead" | "vfs.beginWrite"
            | "vfs.mkdir" | "vfs.rename" | "vfs.trash" | "vfs.remove" | "vfs.search" => {
                parse!(VfsRequest, Vfs)
            }
            "connection.validate" | "connection.open" | "connection.create" => {
                parse!(ConnectionRequest, Connection)
            }
            "hooks.invoke" => parse!(HookRequest, Hook),
            "toolbar.invoke" => parse!(ToolbarRequest, Toolbar),
            _ => Ok(Self::Unspecified),
        }
    }

    pub fn to_params(&self) -> Result<Value, String> {
        match self {
            Self::Unspecified => Ok(Value::Object(Map::new())),
            Self::Resource(value) => serde_json::to_value(value),
            Self::InfoCompute(value) => serde_json::to_value(value),
            Self::ContextMenuResolve(value) => serde_json::to_value(value),
            Self::ContextMenuExecute(value) => serde_json::to_value(value),
            Self::ConnectionMenuResolve(value) => serde_json::to_value(value),
            Self::ConnectionMenuExecute(value) => serde_json::to_value(value),
            Self::Search(value) => serde_json::to_value(value),
            Self::Vfs(value) => serde_json::to_value(value),
            Self::Connection(value) => serde_json::to_value(value),
            Self::Hook(value) => serde_json::to_value(value),
            Self::Toolbar(value) => serde_json::to_value(value),
        }
        .map_err(|error| error.to_string())
    }

    pub fn provider_id(&self) -> Option<&str> {
        match self {
            Self::Resource(value) => value.provider_id.as_deref(),
            Self::InfoCompute(value) => value.provider_id.as_deref(),
            Self::ContextMenuResolve(value) => value.provider_id.as_deref(),
            Self::ContextMenuExecute(value) => value.provider_id.as_deref(),
            Self::ConnectionMenuResolve(value) => value.provider_id.as_deref(),
            Self::ConnectionMenuExecute(value) => value.provider_id.as_deref(),
            Self::Search(value) => value.provider_id.as_deref(),
            Self::Vfs(value) => value.provider_id.as_deref(),
            Self::Connection(value) => value.provider_id.as_deref(),
            Self::Hook(value) => value.provider_id.as_deref(),
            Self::Toolbar(value) => value.provider_id.as_deref(),
            Self::Unspecified => None,
        }
    }

    /// Bind the provider instance selected by Core without serializing and
    /// decoding the event DTO again.
    pub fn set_provider_id(&mut self, provider_id: Option<String>) -> Result<(), String> {
        let field = match self {
            Self::Resource(value) => &mut value.provider_id,
            Self::InfoCompute(value) => &mut value.provider_id,
            Self::ContextMenuResolve(value) => &mut value.provider_id,
            Self::ContextMenuExecute(value) => &mut value.provider_id,
            Self::ConnectionMenuResolve(value) => &mut value.provider_id,
            Self::ConnectionMenuExecute(value) => &mut value.provider_id,
            Self::Search(value) => &mut value.provider_id,
            Self::Vfs(value) => &mut value.provider_id,
            Self::Connection(value) => &mut value.provider_id,
            Self::Hook(value) => &mut value.provider_id,
            Self::Toolbar(value) => &mut value.provider_id,
            Self::Unspecified => {
                return Err("cannot bind provider ID to an unspecified event".into())
            }
        };
        *field = provider_id;
        Ok(())
    }

    /// Add Core-owned session and interaction identity to a file context menu
    /// request without converting the typed request through JSON.
    pub fn set_context_menu_interaction(
        &mut self,
        menu_session_id: Option<String>,
        interaction_id: String,
    ) -> Result<(), String> {
        match self {
            Self::ContextMenuResolve(request) => {
                if let Some(session_id) = menu_session_id {
                    request.menu_session_id = Some(session_id);
                }
                request.interaction_id = Some(interaction_id);
            }
            Self::ContextMenuExecute(request) => {
                if let Some(session_id) = menu_session_id {
                    request.menu_session_id = Some(session_id);
                }
                request.interaction_id = Some(interaction_id);
            }
            _ => return Err("context menu identity requires a context menu request".into()),
        }
        Ok(())
    }

    /// Recheck a ContextMenu action against the same provider's current resolve
    /// result while retaining its session and selected provider identity.
    pub fn context_menu_resolve_for_execute(&self) -> Result<Self, String> {
        let Self::ContextMenuExecute(request) = self else {
            return Err("context menu execute request is required".into());
        };
        Ok(Self::ContextMenuResolve(ContextMenuResolveRequest {
            menu_session_id: request.menu_session_id.clone(),
            interaction_id: request.interaction_id.clone(),
            parent_id: None,
            provider_id: request.provider_id.clone(),
        }))
    }

    pub fn vfs_request(&self) -> Option<&VfsRequest> {
        match self {
            Self::Vfs(request) => Some(request),
            _ => None,
        }
    }

    pub fn connection_request(&self) -> Option<&ConnectionRequest> {
        match self {
            Self::Connection(request) => Some(request),
            _ => None,
        }
    }

    pub fn context_menu_resolve(&self) -> Option<&ContextMenuResolveRequest> {
        match self {
            Self::ContextMenuResolve(request) => Some(request),
            _ => None,
        }
    }

    pub fn context_menu_execute(&self) -> Option<&ContextMenuExecuteRequest> {
        match self {
            Self::ContextMenuExecute(request) => Some(request),
            _ => None,
        }
    }

    pub fn connection_menu_resolve(&self) -> Option<&ConnectionMenuResolveRequest> {
        match self {
            Self::ConnectionMenuResolve(request) => Some(request),
            _ => None,
        }
    }

    pub fn connection_menu_execute(&self) -> Option<&ConnectionMenuExecuteRequest> {
        match self {
            Self::ConnectionMenuExecute(request) => Some(request),
            _ => None,
        }
    }

    pub fn search_request(&self) -> Option<&SearchQueryRequest> {
        match self {
            Self::Search(request) => Some(request),
            _ => None,
        }
    }

    pub fn hook_request(&self) -> Option<&HookRequest> {
        match self {
            Self::Hook(request) => Some(request),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn custom_methods_preserve_arbitrary_params_while_typed_events_require_objects() {
        assert_eq!(
            ProviderInput::decode("example.echo", &json!(["item"])).unwrap(),
            ProviderInput::Unspecified
        );
        assert!(ProviderInput::decode("search.query", &json!(["item"])).is_err());
    }

    #[test]
    fn trash_is_a_typed_vfs_request_method() {
        assert!(matches!(
            ProviderInput::decode("vfs.trash", &json!({"nodeId":"n"})).unwrap(),
            ProviderInput::Vfs(_)
        ));
    }
}
