//! Shared menu contribution DTOs for resource and connection menus.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuItemKind {
    Action,
    Submenu,
    Separator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuPlacement {
    Head,
    Tail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuActivityKind {
    Sync,
    Transfer,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuActivity {
    pub label: String,
    pub kind: MenuActivityKind,
    #[serde(default)]
    pub quiet: bool,
}

/// One flat menu row. Lazy submenu children are requested separately and
/// bound to their parent by Core; providers never return nested `children`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuItem {
    pub id: String,
    pub kind: MenuItemKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub danger: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<MenuPlacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<MenuActivity>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MenuExecuteResult {
    CoreAction(CoreMenuAction),
    ConnectionCoreAction(ConnectionCoreMenuAction),
    RemoteAction(RemoteMenuAction),
    NativeAction(NativeMenuAction),
    HandledKind(MenuHandledKind),
    HandledFlag(MenuHandledFlag),
    Panel(PluginPanelAction),
    /// Provider-specific result data is preserved for the plugin result view.
    Custom(Value),
}

impl MenuExecuteResult {
    pub fn core_action(
        command: impl Into<String>,
        paths: Vec<String>,
        path: impl Into<String>,
        directory: impl Into<String>,
    ) -> Self {
        Self::CoreAction(CoreMenuAction {
            kind: CoreMenuActionKind::CoreAction,
            command: command.into(),
            paths,
            path: path.into(),
            directory: directory.into(),
        })
    }

    pub fn remote_action(
        command: impl Into<String>,
        paths: Vec<String>,
        path: impl Into<String>,
        directory: impl Into<String>,
    ) -> Self {
        Self::RemoteAction(RemoteMenuAction {
            kind: RemoteMenuActionKind::RemoteAction,
            command: command.into(),
            paths,
            path: path.into(),
            directory: directory.into(),
        })
    }

    pub fn connection_core_action(
        command: impl Into<String>,
        connection_id: impl Into<String>,
    ) -> Self {
        Self::ConnectionCoreAction(ConnectionCoreMenuAction {
            kind: CoreMenuActionKind::CoreAction,
            command: command.into(),
            connection_id: connection_id.into(),
        })
    }

    pub fn native_action(item_id: impl Into<String>) -> Self {
        Self::NativeAction(NativeMenuAction {
            kind: NativeMenuActionKind::NativeAction,
            item_id: item_id.into(),
        })
    }

    pub fn handled() -> Self {
        Self::HandledKind(MenuHandledKind {
            kind: MenuHandledKindTag::Handled,
        })
    }

    pub fn handled_flag() -> Self {
        Self::HandledFlag(MenuHandledFlag { handled: true })
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreMenuAction {
    pub kind: CoreMenuActionKind,
    pub command: String,
    pub paths: Vec<String>,
    pub path: String,
    pub directory: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CoreMenuActionKind {
    CoreAction,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteMenuAction {
    pub kind: RemoteMenuActionKind,
    pub command: String,
    pub paths: Vec<String>,
    pub path: String,
    pub directory: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionCoreMenuAction {
    pub kind: CoreMenuActionKind,
    pub command: String,
    pub connection_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteMenuActionKind {
    RemoteAction,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeMenuAction {
    pub kind: NativeMenuActionKind,
    pub item_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeMenuActionKind {
    NativeAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuHandledKind {
    pub kind: MenuHandledKindTag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuHandledKindTag {
    Handled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuHandledFlag {
    pub handled: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPanelAction {
    pub kind: PluginPanelActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<PluginPanelSize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginPanelActionKind {
    Panel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPanelSize {
    pub width: u32,
    pub height: u32,
}

impl MenuItem {
    pub fn action(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: MenuItemKind::Action,
            title: title.into(),
            parent_id: None,
            icon: None,
            shortcut: None,
            disabled: false,
            danger: false,
            placement: None,
            activity: None,
        }
    }

    pub fn submenu(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            kind: MenuItemKind::Submenu,
            ..Self::action(id, title)
        }
    }
}
