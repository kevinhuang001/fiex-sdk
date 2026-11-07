//! Transport-neutral provider request, event routing, and endpoint contracts.
//!
//! Shared by Core-native handlers, Core IPC proxies, and resident SDK
//! implementations. No HostLink, runtime, Tauri or filesystem paths live here.
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
        "info.describe" => ("info", Merge),
        "info.compute" => ("info", Owner),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_event_table_covers_all_provider_families() {
        for (name, extension, dispatch) in [
            ("info.describe", "info", Dispatch::Merge),
            ("info.compute", "info", Dispatch::Owner),
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
}
