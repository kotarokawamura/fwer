// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use crate::config::theme::is_shortcut_disabled;
use crate::i18n::tr;
use crate::menus::types::{MenuAction, MenuContext, MenuEntry};

fn shortcut_opt(spec: &str) -> Option<String> {
    if is_shortcut_disabled(spec) {
        None
    } else {
        Some(spec.to_string())
    }
}

pub fn checkbox_icon(checked: bool) -> String {
    if checked {
        "☑".to_string()
    } else {
        "□".to_string()
    }
}

pub fn default_pane_menu(ctx: &MenuContext) -> Vec<MenuEntry> {
    let sc = &ctx.shortcuts;
    vec![
        MenuEntry::Item {
            icon: "📋".to_string(),
            label: tr("Copy"),
            shortcut: shortcut_opt(&sc.copy),
            action: MenuAction::Copy,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "📥".to_string(),
            label: tr("Paste"),
            shortcut: shortcut_opt(&sc.paste),
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
                    shortcut: shortcut_opt(&sc.split_horizontal),
                    action: MenuAction::SplitHorizontal,
                    enabled: true,
                },
                MenuEntry::Item {
                    icon: "↕".to_string(),
                    label: tr("Split Vertically"),
                    shortcut: shortcut_opt(&sc.split_vertical),
                    action: MenuAction::SplitVertical,
                    enabled: true,
                },
            ],
        },
        MenuEntry::Item {
            icon: "➕".to_string(),
            label: tr("New Tab"),
            shortcut: shortcut_opt(&sc.new_tab),
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
        MenuEntry::Item {
            icon: checkbox_icon(ctx.show_scrollbar),
            label: tr("Toggle Scrollbar"),
            shortcut: shortcut_opt(&sc.toggle_scrollbar),
            action: MenuAction::ToggleScrollbar,
            enabled: true,
        },
        MenuEntry::Item {
            icon: checkbox_icon(ctx.is_read_only),
            label: tr("Read-Only"),
            shortcut: None,
            action: MenuAction::ToggleReadOnly,
            enabled: true,
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            icon: "⚙".to_string(),
            label: tr("Open Settings"),
            shortcut: shortcut_opt(&sc.open_preferences),
            action: MenuAction::OpenSettings,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "✖".to_string(),
            label: tr("Close Pane"),
            shortcut: shortcut_opt(&sc.close_pane),
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
            shortcut: shortcut_opt(&sc.new_tab),
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
                    shortcut: shortcut_opt(&sc.split_horizontal),
                    action: MenuAction::SplitHorizontal,
                    enabled: true,
                },
                MenuEntry::Item {
                    icon: "↕".to_string(),
                    label: tr("Split Vertically"),
                    shortcut: shortcut_opt(&sc.split_vertical),
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
            icon: checkbox_icon(ctx.show_scrollbar),
            label: tr("Toggle Scrollbar"),
            shortcut: shortcut_opt(&sc.toggle_scrollbar),
            action: MenuAction::ToggleScrollbar,
            enabled: true,
        },
        MenuEntry::Item {
            icon: checkbox_icon(ctx.is_read_only),
            label: tr("Read-Only"),
            shortcut: None,
            action: MenuAction::ToggleReadOnly,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "⛶".to_string(),
            label: tr("Toggle Fullscreen"),
            shortcut: shortcut_opt(&sc.toggle_fullscreen),
            action: MenuAction::ToggleFullscreen,
            enabled: true,
        },
        MenuEntry::Item {
            icon: "⚙".to_string(),
            label: tr("Open Settings"),
            shortcut: shortcut_opt(&sc.open_preferences),
            action: MenuAction::OpenSettings,
            enabled: true,
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            icon: "✖".to_string(),
            label: tr("Close Pane"),
            shortcut: shortcut_opt(&sc.close_pane),
            action: MenuAction::ClosePane,
            enabled: true,
        },
    ]
}

pub fn build_presets_submenu(preset_names: &[String]) -> MenuEntry {
    let entries = if preset_names.is_empty() {
        vec![MenuEntry::Item {
            icon: "∅".to_string(),
            label: tr("No Saved Presets"),
            shortcut: None,
            action: MenuAction::OpenSettings,
            enabled: false,
        }]
    } else {
        preset_names
            .iter()
            .map(|name| MenuEntry::Item {
                icon: "▶".to_string(),
                label: name.clone(),
                shortcut: None,
                action: MenuAction::ApplyPreset(name.clone()),
                enabled: true,
            })
            .collect()
    };

    MenuEntry::SubMenu {
        icon: "🗂".to_string(),
        label: tr("Presets"),
        entries,
    }
}
