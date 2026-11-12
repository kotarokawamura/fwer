// Copyright (C) [2026/10/28] kotaro kawamura
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.
pub mod empty_pane;
pub mod interaction;
pub mod pane;
pub mod pane_selector;
pub mod types;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;

use crate::config::theme::ThemeSettings;
use crate::ui::state::{PaneContext, TileId};
use crate::ui::titlebar::TitlebarWidget;

pub use empty_pane::{
    compute_pass_through_hole_rect, is_empty_pane_eof_close_key, set_x11_surface_keep_above,
    EmptyPaneWidget, EMPTY_PANE_FRAME_INSET,
};
pub use pane::{compute_clamped_zoom, encode_terminal_key_bytes, TerminalPaneWidget};
pub use pane_selector::{PaneSelectorWidget, PendingSplitRequest, SelectorKeyAction};
pub use types::{PaneEvent, PaneKind, PaneShortcutAction, SpecialModeKey};

/// Polymorphic wrapper around panel widgets (`TerminalPaneWidget`, `EmptyPaneWidget`, and future panel types).
#[derive(Clone)]
pub enum PaneWidget {
    Terminal(Rc<TerminalPaneWidget>),
    Empty(Rc<EmptyPaneWidget>),
}

impl PaneWidget {
    pub fn new_for_kind<F>(
        kind: PaneKind,
        tile_id: TileId,
        initial_title: &str,
        working_dir: Option<&Path>,
        startup_command: Option<&str>,
        settings: &ThemeSettings,
        on_event: F,
    ) -> Self
    where
        F: Fn(PaneEvent) + 'static,
    {
        match kind {
            PaneKind::Shell => Self::Terminal(TerminalPaneWidget::new(
                tile_id,
                initial_title,
                working_dir,
                startup_command,
                settings,
                on_event,
            )),
            PaneKind::Empty => Self::Empty(EmptyPaneWidget::new(
                tile_id,
                initial_title,
                working_dir,
                settings,
                on_event,
            )),
        }
    }

    pub fn kind(&self) -> PaneKind {
        match self {
            Self::Terminal(_) => PaneKind::Shell,
            Self::Empty(_) => PaneKind::Empty,
        }
    }

    pub fn tile_id(&self) -> TileId {
        match self {
            Self::Terminal(p) => p.tile_id,
            Self::Empty(p) => p.tile_id,
        }
    }

    pub fn root(&self) -> &gtk4::Box {
        match self {
            Self::Terminal(p) => &p.root,
            Self::Empty(p) => &p.root,
        }
    }

    pub fn titlebar(&self) -> &Rc<TitlebarWidget> {
        match self {
            Self::Terminal(p) => &p.titlebar,
            Self::Empty(p) => &p.titlebar,
        }
    }

    pub fn pass_through_widget(&self) -> Option<&gtk4::Widget> {
        match self {
            Self::Terminal(_) => None,
            Self::Empty(p) => Some(p.pass_through_widget()),
        }
    }

    pub fn requires_keep_above(&self) -> bool {
        self.pass_through_widget().is_some()
    }

    pub fn set_keyboard_target(&self, targeted: bool) {
        match self {
            Self::Terminal(p) => p.set_keyboard_target(targeted),
            Self::Empty(p) => p.set_keyboard_target(targeted),
        }
    }

    pub fn set_opacity_highlight(&self, is_source: bool, confirmed: bool) {
        match self {
            Self::Terminal(p) => p.set_opacity_highlight(is_source, confirmed),
            Self::Empty(p) => p.set_opacity_highlight(is_source, confirmed),
        }
    }

    pub fn custom_opacity(&self) -> Option<f32> {
        match self {
            Self::Terminal(p) => p.custom_opacity(),
            Self::Empty(_) => None,
        }
    }

    pub fn set_custom_opacity(&self, opacity: Option<f32>) {
        if let Self::Terminal(p) = self {
            p.set_custom_opacity(opacity);
        }
    }

    pub fn adjust_custom_opacity_by(&self, delta: f32, settings: &ThemeSettings) {
        if let Self::Terminal(p) = self {
            p.adjust_custom_opacity_by(delta, settings);
        }
    }

    pub fn custom_font_size(&self) -> Option<f32> {
        match self {
            Self::Terminal(p) => p.custom_font_size(),
            Self::Empty(_) => None,
        }
    }

    pub fn set_custom_font_size(&self, font_size: Option<f32>) {
        if let Self::Terminal(p) = self {
            p.set_custom_font_size(font_size);
        }
    }

    pub fn custom_show_scrollbar(&self) -> Option<bool> {
        match self {
            Self::Terminal(p) => p.custom_show_scrollbar(),
            Self::Empty(_) => None,
        }
    }

    pub fn effective_show_scrollbar(&self, settings: &ThemeSettings) -> bool {
        match self {
            Self::Terminal(p) => p.effective_show_scrollbar(settings),
            Self::Empty(_) => false,
        }
    }

    pub fn set_custom_show_scrollbar(&self, show: Option<bool>) {
        if let Self::Terminal(p) = self {
            p.set_custom_show_scrollbar(show);
        }
    }

    pub fn toggle_scrollbar(&self) {
        if let Self::Terminal(p) = self {
            p.toggle_scrollbar();
        }
    }

    pub fn is_read_only(&self) -> bool {
        match self {
            Self::Terminal(p) => p.is_read_only(),
            Self::Empty(_) => false,
        }
    }

    pub fn toggle_read_only(&self) {
        if let Self::Terminal(p) = self {
            p.toggle_read_only();
        }
    }

    pub fn startup_command(&self) -> Option<String> {
        match self {
            Self::Terminal(p) => p.startup_command(),
            Self::Empty(_) => None,
        }
    }

    pub fn set_startup_command(&self, cmd: Option<String>) {
        if let Self::Terminal(p) = self {
            p.set_startup_command(cmd);
        }
    }

    pub fn grab_terminal_focus(&self) {
        match self {
            Self::Terminal(p) => p.grab_terminal_focus(),
            Self::Empty(p) => p.grab_terminal_focus(),
        }
    }

    pub fn handle_empty_pane_key_press(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        match self {
            Self::Terminal(_) => false,
            Self::Empty(p) => p.handle_key_press(key, mods),
        }
    }

    pub fn intercept_workspace_key_press(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        self.handle_empty_pane_key_press(key, mods)
    }

    pub fn finish_title_edit_if_active(&self) -> Option<String> {
        match self {
            Self::Terminal(p) => p.finish_title_edit_if_active(),
            Self::Empty(p) => p.finish_title_edit_if_active(),
        }
    }

    pub fn write_input(&self, data: &[u8]) {
        if let Self::Terminal(p) = self {
            p.write_input(data);
        }
    }

    pub fn copy_selection_or_screen(&self) {
        if let Self::Terminal(p) = self {
            p.copy_selection_or_screen();
        }
    }

    pub fn paste_clipboard(&self) {
        if let Self::Terminal(p) = self {
            p.paste_clipboard();
        }
    }

    pub fn clear_scrollback(&self) {
        if let Self::Terminal(p) = self {
            p.clear_scrollback();
        }
    }

    pub fn has_selection(&self) -> bool {
        match self {
            Self::Terminal(p) => p.has_selection(),
            Self::Empty(_) => false,
        }
    }

    pub fn is_alive(&self) -> bool {
        match self {
            Self::Terminal(p) => p.is_alive(),
            Self::Empty(_) => true,
        }
    }

    pub fn detach_callbacks(&self) {
        if let Self::Terminal(p) = self {
            p.detach_callbacks();
        }
    }

    pub fn current_working_dir(&self) -> Option<PathBuf> {
        match self {
            Self::Terminal(p) => p.current_working_dir(),
            Self::Empty(p) => p.current_working_dir(),
        }
    }

    pub fn adjust_zoom_by(&self, delta: f64) {
        if let Self::Terminal(p) = self {
            p.adjust_zoom_by(delta);
        }
    }

    pub fn reset_zoom(&self) {
        if let Self::Terminal(p) = self {
            p.reset_zoom();
        }
    }

    pub fn update_context_and_theme(&self, ctx: &PaneContext, settings: &ThemeSettings) {
        match self {
            Self::Terminal(p) => p.update_context_and_theme(ctx, settings),
            Self::Empty(p) => p.update_context_and_theme(ctx, settings),
        }
    }
}
