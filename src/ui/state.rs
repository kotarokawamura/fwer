// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use crate::config::theme::ShortcutSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId(pub u64);

macro_rules! define_special_modes {
    (
        $($variant:ident => {
                shortcut_field: $sc_field:ident,
                css_class: $css:literal,
                border_rgb: $border_rgb:literal,
                click_selects_pane: $click_sel:literal,
            }
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum InteractionMode {
            Normal,
            $($variant(TileId),
            )*
            EditingTitle(TileId),
            BlockedByModal,
        }

        impl InteractionMode {
            /// pane.rs でハイライトクラスを一括解除するためのスライス
            pub const ALL_SOURCE_CSS_CLASSES: &'static [&'static str] = &[
                $($css, )*
            ];

            /// workspace.rs の update_global_css に埋め込むCSS定義（コンパイル時文字列結合）
            pub const SPECIAL_MODES_CSS: &'static str = concat!(
                $(
                    ".", $css, " {\n",
                    "    border: 2px solid ", $border_rgb, ";\n",
                    "}\n",
                )*
            );

            /// 特殊操作モード中かどうかを判定
            pub fn is_special_mode(&self) -> bool {
                matches!(self, $( Self::$variant(_) )|*)
            }

            /// 特殊操作モード中であれば、そのモードのトグル用ショートカット文字列を返す
            pub fn toggle_shortcut<'a>(&self, sc: &'a ShortcutSettings) -> Option<&'a str> {
                match self {
                    $(
                        Self::$variant(_) => Some(&sc.$sc_field),
                    )*
                    _ => None,
                }
            }

            /// クリック時にペイン選択を行うモード（Move / Broadcast）か、キャンセルするモード（Resize / Opacity）か
            pub fn click_selects_pane(&self) -> bool {
                match self {
                    $(
                        Self::$variant(_) =>$click_sel,
                    )*
                    _ => false,
                }
            }

            /// 指定された tile_id がこのモードの起点ペインであれば対応するCSSクラス名を返す
            pub fn source_css_class(&self, tile_id: TileId) -> Option<&'static str> {
                match self {
                    $(
                        Self::$variant(id) if *id == tile_id => Some($css),
                    )*
                    _ => None,
                }
            }
        }
    };
}

// 特殊操作モードの定義リスト（モード追加時はここに足すだけ）
define_special_modes! {
    MovingPane => {
        shortcut_field: toggle_move_mode,
        css_class: "fwer-pane-moving-source",
        border_rgb: "rgb(250, 204, 21)",
        click_selects_pane: true,
    },
    SelectingBroadcast => {
        shortcut_field: toggle_broadcast_mode,
        css_class: "fwer-pane-broadcast-source",
        border_rgb: "rgb(56, 189, 248)",
        click_selects_pane: true,
    },
    ResizingPane => {
        shortcut_field: toggle_resize_mode,
        css_class: "fwer-pane-resizing-source",
        border_rgb: "rgb(74, 222, 128)",
        click_selects_pane: false,
    },
    AdjustingOpacity => {
        shortcut_field: toggle_opacity_mode,
        css_class: "fwer-pane-opacity-source",
        border_rgb: "rgb(236, 72, 153)",
        click_selects_pane: false,
    },
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