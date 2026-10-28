// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

pub mod presets;
pub mod renderer;
pub mod types;

use crate::menus::presets::{default_pane_menu, default_titlebar_menu};
use crate::menus::types::{MenuContext, MenuEntry, MenuTargetKind};

#[derive(Debug, Clone, Default)]
pub struct MenuManager {
    pane_plugins: Vec<MenuEntry>,
    titlebar_plugins: Vec<MenuEntry>,
}

impl MenuManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_plugin_entry(&mut self, target: MenuTargetKind, entry: MenuEntry) {
        match target {
            MenuTargetKind::Pane => self.pane_plugins.push(entry),
            MenuTargetKind::Titlebar => self.titlebar_plugins.push(entry),
        }
    }

    pub fn build_entries(&self, ctx: &MenuContext) -> Vec<MenuEntry> {
        let (mut base, plugins) = match ctx.target_kind {
            MenuTargetKind::Pane => (default_pane_menu(ctx), &self.pane_plugins),
            MenuTargetKind::Titlebar => (default_titlebar_menu(ctx), &self.titlebar_plugins),
        };
        if !plugins.is_empty() {
            base.push(MenuEntry::Separator);
            base.extend(plugins.iter().cloned());
        }
        base
    }
}

#[cfg(test)]
#[path = "test/renderer_tests.rs"]
mod renderer_tests;
