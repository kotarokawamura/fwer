// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use crate::config::theme::ShortcutSettings;
use crate::ui::state::TileId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuTargetKind {
    Pane,
    Titlebar,
}

#[derive(Debug, Clone)]
pub struct MenuContext {
    pub target_kind: MenuTargetKind,
    pub tile_id: TileId,
    pub has_selection: bool,
    pub shortcuts: ShortcutSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    Copy,
    Paste,
    SplitHorizontal,
    SplitVertical,
    NewTab,
    ClearScrollback,
    SaveLayout,
    LoadLayout,
    ToggleFullscreen,
    OpenSettings,
    ClosePane,
    PluginCommand(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Item {
        icon: String,
        label: String,
        shortcut: Option<String>,
        action: MenuAction,
        enabled: bool,
    },
    SubMenu {
        icon: String,
        label: String,
        entries: Vec<MenuEntry>,
    },
    Separator,
}
