// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

pub mod vt100_adapter;
pub mod vte4_adapter;

use std::path::{Path, PathBuf};

use crate::config::theme::ThemeSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RgbColor(pub u8, pub u8, pub u8);

impl RgbColor {
    pub const DEFAULT_FG: RgbColor = RgbColor(230, 230, 230);
    pub const DEFAULT_BG: RgbColor = RgbColor(18, 18, 18);

    pub fn with_bold_boost(self) -> Self {
        RgbColor(
            self.0.saturating_add(25),
            self.1.saturating_add(25),
            self.2.saturating_add(25),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderableCell {
    pub ch: String,
    pub fg: RgbColor,
    pub bg: RgbColor,
    pub bold: bool,
    pub underline: bool,
    pub inverse: bool,
    pub is_wide: bool,
    pub is_wide_continuation: bool,
}

impl Default for RenderableCell {
    fn default() -> Self {
        Self {
            ch: " ".to_string(),
            fg: RgbColor::DEFAULT_FG,
            bg: RgbColor::DEFAULT_BG,
            bold: false,
            underline: false,
            inverse: false,
            is_wide: false,
            is_wide_continuation: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorState {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
}

/// Standard 16-color One Dark-inspired palette specified in Section 9.3.
pub const ONE_DARK_PALETTE_16: [RgbColor; 16] = [
    RgbColor(40, 44, 52),    // 0: Black
    RgbColor(224, 108, 117), // 1: Red
    RgbColor(152, 195, 121), // 2: Green
    RgbColor(229, 192, 123), // 3: Yellow
    RgbColor(97, 175, 239),  // 4: Blue
    RgbColor(198, 120, 221), // 5: Magenta
    RgbColor(86, 182, 194),  // 6: Cyan
    RgbColor(220, 223, 228), // 7: White
    RgbColor(92, 99, 112),   // 8: Bright Black
    RgbColor(244, 128, 137), // 9: Bright Red
    RgbColor(172, 215, 141), // 10: Bright Green
    RgbColor(249, 212, 143), // 11: Bright Yellow
    RgbColor(117, 195, 255), // 12: Bright Blue
    RgbColor(218, 140, 241), // 13: Bright Magenta
    RgbColor(106, 202, 214), // 14: Bright Cyan
    RgbColor(255, 255, 255), // 15: Bright White
];

/// Converts an 8-bit ANSI color index (0..=255) to 24-bit `RgbColor` according to Section 9.3.
pub fn ansi_idx_to_rgb(idx: u8) -> RgbColor {
    match idx {
        0..=15 => ONE_DARK_PALETTE_16[idx as usize],
        16..=231 => {
            let n = idx - 16;
            let r_idx = n / 36;
            let g_idx = (n % 36) / 6;
            let b_idx = n % 6;
            let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            RgbColor(conv(r_idx), conv(g_idx), conv(b_idx))
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            RgbColor(gray, gray, gray)
        }
    }
}

/// Pluggable terminal backend interface.
///
/// The UI layer (`src/ui/pane.rs`, `src/ui/workspace.rs`, etc.) interacts exclusively
/// with this trait and never references `vte4` directly.
/// To replace `vte4` with another terminal emulator / widget implementation later,
/// implement `TerminalBackend` and return it from [`create_terminal_backend`].
pub trait TerminalBackend {
    /// Returns the GTK4 widget to attach inside the pane container.
    fn widget(&self) -> gtk4::Widget;

    /// Spawns a shell (or `startup_command` via `$SHELL -c`) in `working_dir`.
    fn spawn_shell(
        &mut self,
        working_dir: Option<&Path>,
        startup_command: Option<&str>,
    ) -> Result<(), String>;

    /// Sends raw input bytes to the terminal child process.
    fn write_input(&mut self, data: &[u8]);

    /// Feeds raw output bytes into the terminal state machine.
    fn process_bytes(&mut self, data: &[u8]);

    /// Returns true if the user currently has text selected in the terminal.
    fn has_selection(&self) -> bool;

    /// Copies the selected text to the system clipboard, or copies the visible screen
    /// if no selection is active.
    fn copy_selection_or_screen(&self);

    /// Pastes text from the system clipboard into the terminal.
    fn paste_clipboard(&self);

    /// Clears scrollback history while keeping current visible lines.
    fn clear_scrollback(&mut self);

    /// Scrolls the viewport by `delta_lines` (negative = up into history, positive = down).
    fn scroll_lines(&mut self, delta_lines: i32);

    /// Scrolls the viewport by half a page (`up = true` scrolls up into history).
    fn scroll_page(&mut self, up: bool);

    /// Scrolls to the very top of the scrollback buffer.
    fn scroll_to_top(&mut self);

    /// Scrolls to the bottom (live screen).
    fn scroll_to_bottom(&mut self);

    /// Returns `(current_scroll_offset, max_scroll_offset)` measured in lines from the bottom.
    fn scroll_offset_info(&self) -> (usize, usize);

    /// Returns the vertical `gtk4::Adjustment` for binding a `gtk4::Scrollbar`.
    fn vadjustment(&self) -> Option<gtk4::Adjustment>;

    /// Applies colors, opacity, and font settings from `ThemeSettings`.
    fn apply_theme(&mut self, settings: &ThemeSettings);

    /// Updates the per-pane zoom scale (`0.4..=3.5`).
    fn set_zoom_scale(&mut self, scale: f64);

    /// Returns the current per-pane zoom scale.
    fn zoom_scale(&self) -> f64;

    /// Returns the current working directory of the foreground process in the terminal.
    fn current_working_dir(&self) -> Option<PathBuf>;

    /// Returns whether the child shell process is still running.
    fn is_alive(&self) -> bool;

    /// Returns `(rows, cols)` of the terminal grid.
    fn dimensions(&self) -> (u16, u16);

    /// Sets callback triggered when the user inputs bytes directly into this terminal.
    fn set_on_user_input(&mut self, cb: Box<dyn Fn(&[u8]) + 'static>);

    /// Sets callback triggered when the child process exits.
    fn set_on_child_exited(&mut self, cb: Box<dyn Fn(i32) + 'static>);

    /// Sets callback triggered when scroll offset or scrollback length changes.
    fn set_on_scroll_changed(&mut self, cb: Box<dyn Fn(usize, usize) + 'static>);
}

/// Factory function for creating the active terminal backend.
/// When swapping out `vte4` in the future, only this factory function needs to be changed.
pub fn create_terminal_backend(settings: &ThemeSettings) -> Box<dyn TerminalBackend> {
    Box::new(vte4_adapter::Vte4Adapter::new(settings))
}
