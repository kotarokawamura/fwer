// Copyright (C) [2026/10/28] kotaro kawamura
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.
// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use serde::{Deserialize, Serialize};

pub use crate::config::theme::PaneShortcutAction;
use crate::i18n::tr;
use crate::ui::state::TileId;
use crate::ui::titlebar::TitlebarEvent;
use crate::ui::tree_ops::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaneKind {
    #[default]
    Shell,
    Empty,
}

impl PaneKind {
    pub const ALL: &'static [PaneKind] = &[PaneKind::Shell, PaneKind::Empty];

    pub fn id_str(self) -> &'static str {
        match self {
            PaneKind::Shell => "shell",
            PaneKind::Empty => "empty",
        }
    }

    pub fn from_str_opt(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "shell" | "terminal" | "normal" | "default" => Some(PaneKind::Shell),
            "empty" | "blank" | "passthrough" | "pass-through" | "frame" => Some(PaneKind::Empty),
            _ => None,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            PaneKind::Shell => "🖥",
            PaneKind::Empty => "⬜",
        }
    }

    pub fn display_name(self) -> String {
        match self {
            PaneKind::Shell => tr("Shell Panel"),
            PaneKind::Empty => tr("Empty Panel"),
        }
    }

    pub fn description(self) -> String {
        match self {
            PaneKind::Shell => tr("Standard terminal shell"),
            PaneKind::Empty => tr("Frame only — interact with apps behind"),
        }
    }

    pub fn default_title(self) -> String {
        match self {
            PaneKind::Shell => std::env::var("SHELL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "/bin/bash".to_string()),
            PaneKind::Empty => tr("Empty"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialModeKey {
    Move(Direction),
    AdjustOpacity(i8),
    Confirm,
    KeepSelecting,
    Cancel,
}

#[derive(Debug, Clone)]
pub enum PaneEvent {
    Titlebar(TitlebarEvent),
    ClickedInSpecialMode {
        tile_id: TileId,
        keep_selecting: bool,
    },
    OpacityScroll {
        tile_id: TileId,
        delta: f32,
    },
    OpenPaneContextMenu {
        tile_id: TileId,
        x: f64,
        y: f64,
    },
    ShortcutTriggered {
        tile_id: TileId,
        action: PaneShortcutAction,
    },
    SpecialModeKeyInput {
        tile_id: TileId,
        key: SpecialModeKey,
    },
    CancelSpecialMode,
    BroadcastInput {
        source_id: TileId,
        bytes: Vec<u8>,
    },
    ChildExited(TileId),
}
