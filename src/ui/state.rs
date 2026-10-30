// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMode {
    Normal,
    MovingPane(TileId),
    SelectingBroadcast(TileId),
    ResizingPane(TileId),
    EditingTitle(TileId),
    BlockedByModal,
}

impl Default for InteractionMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneContext {
    pub tile_id: TileId,
    pub is_active: bool,
    pub is_top_right: bool,
    pub broadcast_group: usize,
    pub mode: InteractionMode,
}

impl PaneContext {
    pub fn is_move_source(&self) -> bool {
        self.mode == InteractionMode::MovingPane(self.tile_id)
    }

    pub fn is_broadcast_source(&self) -> bool {
        self.mode == InteractionMode::SelectingBroadcast(self.tile_id)
    }
    pub fn is_resize_source(&self) -> bool {
        self.mode == InteractionMode::ResizingPane(self.tile_id)
    }
    pub fn is_editing_this_title(&self) -> bool {
        self.mode == InteractionMode::EditingTitle(self.tile_id)
    }

    /// Returns true if terminal interactions (typing/selecting) should be blocked on this pane.
    pub fn is_terminal_input_blocked(&self) -> bool {
        !matches!(self.mode, InteractionMode::Normal)
    }

    /// Returns true if other panes' titlebars/controls are blocked.
    pub fn is_other_pane_blocked(&self) -> bool {
        match self.mode {
            InteractionMode::BlockedByModal => true,
            InteractionMode::EditingTitle(editing_id) => editing_id != self.tile_id,
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "test/state_tests.rs"]
mod state_tests;
