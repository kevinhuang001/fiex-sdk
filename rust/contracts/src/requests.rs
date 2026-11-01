//! Typed input contracts for standard provider events.
//!
//! JSON-RPC parameters are decoded into these event-specific DTOs at the
//! transport boundary. Provider implementations never receive an untyped
//! parameter object through `ProviderInvocation`.

use crate::{ResourceRef, SearchMode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceEventRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextMenuResolveRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextMenuExecuteRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionMenuResolveRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionMenuExecuteRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSummary>,
}

/// Credentials are projected out before a connection request is sent. The
/// remaining summary is descriptive data only and never grants a capability.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub plugin_id: String,
    #[serde(default)]
    pub mount_path: String,
    #[serde(default)]
    pub saved: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub plugin_view: bool,
    #[serde(default)]
    pub vfs: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ConnectionFieldValue {
    String(String),
    Boolean(bool),
    Number(serde_json::Number),
    /// Core's effective network allowlist is added to connection config as a
    /// string array; declared user-editable fields remain scalar values.
    Strings(Vec<String>),
}

pub type ConnectionSettings = BTreeMap<String, ConnectionFieldValue>;

impl ConnectionSettings {
    pub fn to_value(&self) -> Result<Value, String> {
        serde_json::to_value(self).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

impl ConnectionRequest {
    pub fn connection_value(&self) -> Result<Option<Value>, String> {
        self.connection.as_ref().map(ConnectionSettings::to_value).transpose()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchQueryRequest {
    #[serde(default)]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<ResourceRef>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default)]
    pub ignores: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

impl SearchQueryRequest {
    pub fn bounded_limit(&self) -> usize {
        self.limit.unwrap_or(500).clamp(1, 500)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookRequest {
    #[serde(default)]
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolbarRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

/// Typed storage request fields used by Core and the SDK's VFS dispatcher.
/// Stream chunk payloads remain private transport data in `FsServer`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VfsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_revision: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub ignores: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SearchMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_context: Option<BTreeMap<String, Value>>,
}

impl VfsRequest {
    pub fn connection_value(&self) -> Result<Option<Value>, String> {
        self.connection.as_ref().map(ConnectionSettings::to_value).transpose()
    }
}

impl VfsRequest {
    pub fn bounded_limit(&self, maximum: usize) -> usize {
        self.limit.unwrap_or(maximum).clamp(1, maximum)
    }
}

/// A decoded standard event. `Unspecified` is used only by custom RPC and
/// VFS calls, whose transport contracts are separate from provider events.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ProviderInput {
    #[default]
    Unspecified,
    Resource(ResourceEventRequest),
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
        let mut object = params
            .as_object()
            .cloned()
            .ok_or_else(|| "provider event parameters must be an object".to_string())?;
        for key in ["targets", "directory", "selectionCount", "sessionId", "locale"] {
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
            "info.resolve" | "preview.open" | "decorations.resolve" => {
                parse!(ResourceEventRequest, Resource)
            }
            "contextMenu.resolve" => parse!(ContextMenuResolveRequest, ContextMenuResolve),
            "contextMenu.execute" => parse!(ContextMenuExecuteRequest, ContextMenuExecute),
            "connectionContextMenu.resolve" => {
                parse!(ConnectionMenuResolveRequest, ConnectionMenuResolve)
            }
            "connectionContextMenu.execute" => {
                parse!(ConnectionMenuExecuteRequest, ConnectionMenuExecute)
            }
            "search.query" => parse!(SearchQueryRequest, Search),
            "vfs.discover" | "vfs.list" | "vfs.stat" | "vfs.openRead" | "vfs.beginWrite" |
            "vfs.mkdir" | "vfs.rename" | "vfs.remove" | "vfs.search" => {
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
            Self::ContextMenuResolve(value) => &mut value.provider_id,
            Self::ContextMenuExecute(value) => &mut value.provider_id,
            Self::ConnectionMenuResolve(value) => &mut value.provider_id,
            Self::ConnectionMenuExecute(value) => &mut value.provider_id,
            Self::Search(value) => &mut value.provider_id,
            Self::Vfs(value) => &mut value.provider_id,
            Self::Connection(value) => &mut value.provider_id,
            Self::Hook(value) => &mut value.provider_id,
            Self::Toolbar(value) => &mut value.provider_id,
            Self::Unspecified => return Err("cannot bind provider ID to an unspecified event".into()),
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
        match self { Self::Vfs(request) => Some(request), _ => None }
    }

    pub fn connection_request(&self) -> Option<&ConnectionRequest> {
        match self { Self::Connection(request) => Some(request), _ => None }
    }

    pub fn context_menu_resolve(&self) -> Option<&ContextMenuResolveRequest> {
        match self { Self::ContextMenuResolve(request) => Some(request), _ => None }
    }

    pub fn context_menu_execute(&self) -> Option<&ContextMenuExecuteRequest> {
        match self { Self::ContextMenuExecute(request) => Some(request), _ => None }
    }

    pub fn connection_menu_resolve(&self) -> Option<&ConnectionMenuResolveRequest> {
        match self { Self::ConnectionMenuResolve(request) => Some(request), _ => None }
    }

    pub fn connection_menu_execute(&self) -> Option<&ConnectionMenuExecuteRequest> {
        match self { Self::ConnectionMenuExecute(request) => Some(request), _ => None }
    }

    pub fn search_request(&self) -> Option<&SearchQueryRequest> {
        match self { Self::Search(request) => Some(request), _ => None }
    }

    pub fn hook_request(&self) -> Option<&HookRequest> {
        match self { Self::Hook(request) => Some(request), _ => None }
    }
}
