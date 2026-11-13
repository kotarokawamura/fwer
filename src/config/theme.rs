// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::vte::RgbColor;

// 既存モジュール（ui/, pane/, menus/, config/, tests）からの `crate::config::theme::*` 参照を維持
pub use crate::config::font_scanner::*;
pub use crate::config::shortcuts::*;

/// アプリケーション設定ディレクトリの相対パス（HOMEからの相対・一元管理用）
pub const CONFIG_DIR_REL: &str = ".config/fwer";

/// 旧バージョン設定ディレクトリのフォールバック検索順
pub const LEGACY_CONFIG_DIR_RELS: &[&str] = &[".config/termin", ".config/termini"];

static CUSTOM_CONFIG_PATH: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

fn custom_config_lock() -> &'static RwLock<Option<PathBuf>> {
    CUSTOM_CONFIG_PATH.get_or_init(|| RwLock::new(None))
}

pub fn set_custom_config_path(path: Option<PathBuf>) {
    if let Ok(mut guard) = custom_config_lock().write() {
        *guard = path;
    }
}

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
}

/// 基準となる設定ディレクトリ（カスタムパス指定時はその親ディレクトリ、通常時は `~/.config/fwer`）を返します。
pub fn default_config_dir() -> PathBuf {
    if let Ok(guard) = custom_config_lock().read()
        && let Some(ref custom) = *guard
        && let Some(parent) = custom.parent()
    {
        return parent.to_path_buf();
    }
    home_dir().join(CONFIG_DIR_REL)
}

pub fn config_path() -> PathBuf {
    if let Ok(guard) = custom_config_lock().read()
        && let Some(ref custom) = *guard
    {
        return custom.clone();
    }
    default_config_dir().join("config.json")
}

pub fn legacy_config_paths() -> Vec<PathBuf> {
    let home = home_dir();
    LEGACY_CONFIG_DIR_RELS
        .iter()
        .map(|rel| home.join(rel).join("config.json"))
        .collect()
}

pub fn legacy_config_path() -> PathBuf {
    home_dir()
        .join(LEGACY_CONFIG_DIR_RELS[0])
        .join("config.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThemeSettings {
    pub title_active_bg: String,
    pub title_active_fg: String,
    pub title_inactive_bg: String,
    pub title_inactive_fg: String,
    pub frame_bg_color: String,
    pub frame_border_color: String,
    pub separator_color: String,
    pub separator_border_color: String,
    pub terminal_bg_color: String,
    pub cursor_color: String,
    pub selection_bg_color: String,
    pub terminal_bg_opacity: f32,
    pub separator_width: f32,
    pub frame_border_width: f32,
    pub target_fps: u32,
    pub show_scrollbar: bool,
    pub select_pane_type_on_split: bool,
    pub select_pane_type_timeout_secs: f32,
    pub font_path: String,
    pub ui_font_size: f32,
    pub terminal_font_size: f32,
    pub language: String,
    pub custom_po_path: String,
    pub shortcuts: ShortcutSettings,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            title_active_bg: String::new(),
            title_active_fg: String::new(),
            title_inactive_bg: String::new(),
            title_inactive_fg: String::new(),
            frame_bg_color: "#2b2b2b".to_string(),
            frame_border_color: "#5a5a5a".to_string(),
            separator_color: String::new(),
            separator_border_color: String::new(),
            terminal_bg_color: "#000000".to_string(),
            cursor_color: String::new(),
            selection_bg_color: String::new(),
            terminal_bg_opacity: 1.0,
            separator_width: 2.0,
            frame_border_width: 5.0,
            target_fps: 30,
            show_scrollbar: true,
            select_pane_type_on_split: false,
            select_pane_type_timeout_secs: 1.5,
            font_path: default_debian_cjk_fallback(),
            ui_font_size: 15.0,
            terminal_font_size: 15.0,
            language: "auto".to_string(),
            custom_po_path: String::new(),
            shortcuts: ShortcutSettings::default(),
        }
    }
}

/// Snaps a panel-type selector timer value to `0.5`-second increments in `0.0..=10.0`.
/// `0.0` disables the automatic confirmation timer.
pub fn snap_half_second_timeout(val: f32) -> f32 {
    if !val.is_finite() || val <= 0.0 {
        0.0
    } else {
        ((val.clamp(0.0, 10.0) * 2.0).round()) / 2.0
    }
}

impl ThemeSettings {
    pub fn sanitize(&mut self) {
        self.terminal_bg_opacity = self.terminal_bg_opacity.clamp(0.0, 1.0);
        self.separator_width = self.separator_width.clamp(1.0, 16.0);
        self.frame_border_width = self.frame_border_width.clamp(1.0, 20.0);
        if self.target_fps == 0 {
            self.target_fps = 30;
        } else {
            self.target_fps = self.target_fps.clamp(1, 240);
        }
        self.select_pane_type_timeout_secs =
            snap_half_second_timeout(self.select_pane_type_timeout_secs);
        self.ui_font_size = self.ui_font_size.clamp(8.0, 32.0);
        self.terminal_font_size = self.terminal_font_size.clamp(8.0, 48.0);
        if self.language.trim().is_empty() {
            self.language = "auto".to_string();
        }
    }

    pub fn load(json_override: Option<&str>) -> Self {
        let primary = config_path();
        let raw_content = fs::read_to_string(&primary).or_else(|e| {
            for legacy in legacy_config_paths() {
                if let Ok(text) = fs::read_to_string(&legacy) {
                    return Ok(text);
                }
            }
            Err(e)
        });

        let mut base_val: Value = match raw_content {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
                serde_json::to_value(ThemeSettings::default()).unwrap_or(Value::Null)
            }),
            Err(_) => serde_json::to_value(ThemeSettings::default()).unwrap_or(Value::Null),
        };

        if let Some(patch_arg) = json_override {
            let trimmed = patch_arg.trim();
            if !trimmed.is_empty() {
                let patch_str = if !trimmed.starts_with('{') && Path::new(trimmed).exists() {
                    fs::read_to_string(trimmed).unwrap_or_else(|_| trimmed.to_string())
                } else {
                    trimmed.to_string()
                };
                if let Ok(patch_val) = serde_json::from_str::<Value>(&patch_str) {
                    merge_json_values(&mut base_val, &patch_val);
                }
            }
        }

        let mut settings: ThemeSettings = serde_json::from_value(base_val).unwrap_or_default();
        settings.sanitize();
        settings
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn effective_title_active_bg(&self) -> RgbColor {
        parse_color(&self.title_active_bg).unwrap_or(RgbColor(117, 33, 0))
    }

    pub fn effective_title_active_fg(&self) -> RgbColor {
        parse_color(&self.title_active_fg).unwrap_or(RgbColor(255, 255, 255))
    }

    pub fn effective_title_inactive_bg(&self) -> RgbColor {
        parse_color(&self.title_inactive_bg).unwrap_or(RgbColor(75, 75, 75))
    }

    pub fn effective_title_inactive_fg(&self) -> RgbColor {
        parse_color(&self.title_inactive_fg).unwrap_or(RgbColor(210, 210, 210))
    }

    pub fn effective_frame_bg_rgb(&self) -> RgbColor {
        parse_color(&self.frame_bg_color).unwrap_or(RgbColor(43, 43, 43))
    }

    pub fn effective_frame_border_rgb(&self) -> RgbColor {
        parse_color(&self.frame_border_color).unwrap_or(RgbColor(90, 90, 90))
    }

    pub fn effective_separator_rgb(&self) -> RgbColor {
        parse_color(&self.separator_color).unwrap_or(RgbColor(74, 74, 74))
    }

    pub fn effective_separator_border_rgb(&self) -> RgbColor {
        parse_color(&self.separator_border_color).unwrap_or_else(|| self.effective_separator_rgb())
    }

    pub fn effective_terminal_bg_rgb(&self) -> RgbColor {
        parse_color(&self.terminal_bg_color).unwrap_or(RgbColor(0, 0, 0))
    }

    pub fn effective_cursor_rgb(&self) -> RgbColor {
        parse_color(&self.cursor_color).unwrap_or(RgbColor(220, 220, 220))
    }

    pub fn effective_selection_bg_rgb(&self) -> RgbColor {
        parse_color(&self.selection_bg_color).unwrap_or(RgbColor(230, 230, 230))
    }

    pub fn effective_terminal_bg_opacity(&self) -> f32 {
        self.terminal_bg_opacity.clamp(0.0, 1.0)
    }

    pub fn effective_separator_width(&self) -> f32 {
        self.separator_width.clamp(1.0, 16.0)
    }

    pub fn effective_border_width(&self) -> f32 {
        self.frame_border_width.clamp(1.0, 20.0)
    }

    pub fn effective_target_fps(&self) -> u32 {
        if self.target_fps == 0 {
            30
        } else {
            self.target_fps.clamp(1, 240)
        }
    }

    pub fn effective_select_pane_type_timeout_secs(&self) -> f32 {
        snap_half_second_timeout(self.select_pane_type_timeout_secs)
    }

    pub fn effective_ui_font_size(&self) -> f32 {
        self.ui_font_size.clamp(8.0, 32.0)
    }

    pub fn effective_terminal_font_size(&self) -> f32 {
        self.terminal_font_size.clamp(8.0, 48.0)
    }

    pub fn effective_font_family(&self) -> String {
        extract_font_family_from_path(&self.font_path).unwrap_or_else(|| "Monospace".to_string())
    }
}

pub fn merge_json_values(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (k, v) in patch_map {
                merge_json_values(base_map.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (base_slot, patch_val) => {
            *base_slot = patch_val.clone();
        }
    }
}

/// Parses `#rrggbb` or `rgb(r, g, b)` color strings (Section 4.2).
pub fn parse_color(input: &str) -> Option<RgbColor> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(RgbColor(r, g, b));
        }
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if let Some(inner) = lower.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some(RgbColor(r, g, b));
        }
    }
    None
}

pub fn format_rgb_hex(c: RgbColor) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// Returns the 6-color cycle palette for broadcast group titlebars (Section 4.2).
pub fn broadcast_group_title_bg(group_id: usize, is_active: bool) -> RgbColor {
    let idx = group_id.saturating_sub(1) % 6;
    match (idx, is_active) {
        (0, true) => RgbColor(25, 118, 210),
        (0, false) => RgbColor(21, 75, 135),
        (1, true) => RgbColor(16, 142, 92),
        (1, false) => RgbColor(18, 88, 60),
        (2, true) => RgbColor(138, 58, 185),
        (2, false) => RgbColor(86, 40, 116),
        (3, true) => RgbColor(217, 119, 6),
        (3, false) => RgbColor(138, 78, 12),
        (4, true) => RgbColor(0, 151, 167),
        (4, false) => RgbColor(10, 93, 104),
        (_, true) => RgbColor(194, 24, 91),
        (_, false) => RgbColor(120, 22, 62),
    }
}

#[cfg(test)]
#[path = "test/prefs_ui_tests.rs"]
mod prefs_ui_tests;
