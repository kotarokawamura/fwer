// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use crate::i18n::tr;
use crate::menus::types::{MenuAction, MenuContext, MenuEntry};

pub fn default_pane_menu(ctx: &MenuContext) -> Vec<MenuEntry> {
    let sc = &ctx.shortcuts;
    vec![
        MenuEntry::Item {
            icon: "📋".to_string(),
            label: tr("Copy"),
            shortcut: Some(sc.copy.clone()),
            action: MenuAction::Copy,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "📥".to_string(),
            label: tr("Paste"),
            shortcut: Some(sc.paste.clone()),
            action: MenuAction::Paste,
            enabled: true,
        },
        MenuEntry::Separator,
        MenuEntry::SubMenu {
            icon: "🪟".to_string(),
            label: tr("Split Pane"),
            entries: vec![
                MenuEntry::Item {
                    icon: "↔".to_string(),
                    label: tr("Split Horizontally"),
                    shortcut: Some(sc.split_horizontal.clone()),
                    action: MenuAction::SplitHorizontal,
                    enabled: true,
                },
                MenuEntry::Item {
                    icon: "↕".to_string(),
                    label: tr("Split Vertically"),
                    shortcut: Some(sc.split_vertical.clone()),
                    action: MenuAction::SplitVertical,
                    enabled: true,
                },
            ],
        },
        MenuEntry::Item {
            icon: "➕".to_string(),
            label: tr("New Tab"),
            shortcut: Some(sc.new_tab.clone()),
            action: MenuAction::NewTab,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "🧹".to_string(),
            label: tr("Clear Scrollback"),
            shortcut: None,
            action: MenuAction::ClearScrollback,
            enabled: true,
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            icon: "⚙".to_string(),
            label: tr("Open Settings"),
            shortcut: Some(sc.open_preferences.clone()),
            action: MenuAction::OpenSettings,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "✖".to_string(),
            label: tr("Close Pane"),
            shortcut: Some(sc.close_pane.clone()),
            action: MenuAction::ClosePane,
            enabled: true,
        },
    ]
}

pub fn default_titlebar_menu(ctx: &MenuContext) -> Vec<MenuEntry> {
    let sc = &ctx.shortcuts;
    vec![
        MenuEntry::Item {
            icon: "➕".to_string(),
            label: tr("New Tab"),
            shortcut: Some(sc.new_tab.clone()),
            action: MenuAction::NewTab,
            enabled: true,
        },
        MenuEntry::SubMenu {
            icon: "🪟".to_string(),
            label: tr("Split Pane"),
            entries: vec![
                MenuEntry::Item {
                    icon: "↔".to_string(),
                    label: tr("Split Horizontally"),
                    shortcut: Some(sc.split_horizontal.clone()),
                    action: MenuAction::SplitHorizontal,
                    enabled: true,
                },
                MenuEntry::Item {
                    icon: "↕".to_string(),
                    label: tr("Split Vertically"),
                    shortcut: Some(sc.split_vertical.clone()),
                    action: MenuAction::SplitVertical,
                    enabled: true,
                },
            ],
        },
        MenuEntry::SubMenu {
            icon: "💾".to_string(),
            label: tr("Layout"),
            entries: vec![
                MenuEntry::Item {
                    icon: "💾".to_string(),
                    label: tr("Save Current Layout"),
                    shortcut: None,
                    action: MenuAction::SaveLayout,
                    enabled: true,
                },
                MenuEntry::Item {
                    icon: "📂".to_string(),
                    label: tr("Load Saved Layout"),
                    shortcut: None,
                    action: MenuAction::LoadLayout,
                    enabled: true,
                },
            ],
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            icon: "⛶".to_string(),
            label: tr("Toggle Fullscreen"),
            shortcut: Some(sc.toggle_fullscreen.clone()),
            action: MenuAction::ToggleFullscreen,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "⚙".to_string(),
            label: tr("Open Settings"),
            shortcut: Some(sc.open_preferences.clone()),
            action: MenuAction::OpenSettings,
            enabled: true,
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            icon: "✖".to_string(),
            label: tr("Close Pane"),
            shortcut: Some(sc.close_pane.clone()),
            action: MenuAction::ClosePane,
            enabled: true,
        },
    ]
}
