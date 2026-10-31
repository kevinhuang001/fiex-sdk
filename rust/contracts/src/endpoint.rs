//! Transport-neutral provider request, event routing, and endpoint contracts.
//!
//! Shared by Core-native handlers, Core IPC proxies, and resident SDK
//! implementations. No HostLink, runtime, Tauri or filesystem paths live here.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One dispatch strategy per event. It is independent of whether the provider
/// is executed in Core or via the SDK's framed JSON-RPC dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dispatch {
    Merge,
    FirstSupported,
    Owner,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub extension: &'static str,
    pub dispatch: Dispatch,
}

impl Event {
    /// One canonical event-family identity for descriptor and IPC validation.
    pub fn kind(self) -> Option<crate::ProviderKind> {
        use crate::ProviderKind;
        match self.extension {
            "info" => Some(ProviderKind::Info),
            "preview" => Some(ProviderKind::Preview),
            "decorations" => Some(ProviderKind::Decoration),
            "contextMenus" => Some(ProviderKind::ContextMenu),
            "search" => Some(ProviderKind::Search),
            "toolbar" => Some(ProviderKind::Toolbar),
            "hooks" => Some(ProviderKind::Hook),
            "connections" => Some(ProviderKind::Connection),
            "connectionContextMenus" => Some(ProviderKind::ConnectionContextMenu),
            _ => None,
        }
    }
}

pub fn event(method: &str) -> Option<Event> {
    use Dispatch::*;
    let (extension, dispatch) = match method {
        "info.resolve" => ("info", Merge),
        "preview.open" => ("preview", FirstSupported),
        "decorations.resolve" => ("decorations", Merge),
        "contextMenu.resolve" => ("contextMenus", Merge),
        "contextMenu.execute" => ("contextMenus", Owner),
        "search.query" => ("search", Merge),
        "toolbar.invoke" => ("toolbar", Owner),
        "hooks.invoke" => ("hooks", FirstSupported),
        "connection.open" | "connection.create" | "connection.validate" => ("connections", Owner),
        "connectionContextMenu.resolve" => ("connectionContextMenus", Merge),
        "connectionContextMenu.execute" => ("connectionContextMenus", Owner),
        _ => return None,
    };
    Some(Event {
        extension,
        dispatch,
    })
}

/// A transport-neutral request. No direct host path, authentication token,
/// plugin ID, or user-derived ownership claim is included here: those values
/// belong to the authorized Core invocation scope, not provider input.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequest {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// One contract that a Core-native handler and an IPC plugin proxy can both
/// implement. Implementations must never broaden the pre-authorized scope.
/// Feature-specific typed providers may sit behind this common boundary.
pub trait ProviderEndpoint: Send + Sync {
    fn invoke(&self, request: &ProviderRequest) -> Result<Value, String>;
}

/// Registered, shared endpoint instances use the same runtime-neutral
/// dispatch contract as stack-allocated implementations.
impl<T: ProviderEndpoint + ?Sized> ProviderEndpoint for std::sync::Arc<T> {
    fn invoke(&self, request: &ProviderRequest) -> Result<Value, String> {
        (**self).invoke(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Echo;
    impl ProviderEndpoint for Echo {
        fn invoke(&self, request: &ProviderRequest) -> Result<Value, String> {
            Ok(json!({"method":request.method,"params":request.params}))
        }
    }

    #[test]
    fn public_event_table_covers_all_provider_families() {
        for (name, extension, dispatch) in [
            ("info.resolve", "info", Dispatch::Merge),
            ("preview.open", "preview", Dispatch::FirstSupported),
            ("decorations.resolve", "decorations", Dispatch::Merge),
            ("contextMenu.resolve", "contextMenus", Dispatch::Merge),
            ("contextMenu.execute", "contextMenus", Dispatch::Owner),
            ("search.query", "search", Dispatch::Merge),
            ("hooks.invoke", "hooks", Dispatch::FirstSupported),
            ("toolbar.invoke", "toolbar", Dispatch::Owner),
            ("connection.validate", "connections", Dispatch::Owner),
            (
                "connectionContextMenu.resolve",
                "connectionContextMenus",
                Dispatch::Merge,
            ),
            (
                "connectionContextMenu.execute",
                "connectionContextMenus",
                Dispatch::Owner,
            ),
        ] {
            let kind = event(name).expect("registered method");
            assert_eq!(kind.extension, extension);
            assert_eq!(kind.dispatch, dispatch);
            assert!(kind.kind().is_some(), "{name} has no provider kind");
        }
        assert!(event("fs.read").is_none());
        assert!(event("custom.example").is_none());
    }

    #[test]
    fn both_transports_share_one_endpoint_signature() {
        let endpoint: &dyn ProviderEndpoint = &Echo;
        let call = ProviderRequest {
            method: "info.resolve".into(),
            params: json!({"resource":"opaque"}),
        };
        assert_eq!(
            endpoint.invoke(&call).unwrap(),
            json!({"method":"info.resolve","params":{"resource":"opaque"}})
        );
    }
}
