// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};

use gtk4::gdk;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::vte::RgbColor;

static CUSTOM_CONFIG_PATH: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

fn custom_config_lock() -> &'static RwLock<Option<PathBuf>> {
    CUSTOM_CONFIG_PATH.get_or_init(|| RwLock::new(None))
}

pub fn set_custom_config_path(path: Option<PathBuf>) {
    if let Ok(mut guard) = custom_config_lock().write() {
        *guard = path;
    }
}

pub fn config_path() -> PathBuf {
    if let Ok(guard) = custom_config_lock().read() {
        if let Some(ref custom) = *guard {
            return custom.clone();
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/termin/config.json")
}

pub fn legacy_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/termini/config.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ShortcutSettings {
    pub copy: String,
    pub paste: String,
    pub split_horizontal: String,
    pub split_vertical: String,
    pub new_tab: String,
    pub close_pane: String,
    pub focus_next_pane: String,
    pub focus_prev_pane: String,
    pub open_preferences: String,
    pub close_preferences: String,
    pub toggle_collapse_preferences: String,
    pub toggle_fullscreen: String,
    pub zoom_in: String,
    pub zoom_out: String,
    pub zoom_reset: String,
    #[serde(default = "default_toggle_move_mode")]
    pub toggle_move_mode: String,
    #[serde(default = "default_toggle_resize_mode")]
    pub toggle_resize_mode: String,
    #[serde(default = "default_toggle_broadcast_mode")]
    pub toggle_broadcast_mode: String,
}

fn default_toggle_move_mode() -> String {
    "Ctrl+Shift+M".to_string()
}
fn default_toggle_resize_mode() -> String {
    "Ctrl+Shift+R".to_string()
}
fn default_toggle_broadcast_mode() -> String {
    "Ctrl+Shift+B".to_string()
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            copy: "Ctrl+Shift+C".to_string(),
            paste: "Ctrl+Shift+V".to_string(),
            split_horizontal: "Ctrl+Shift+E".to_string(),
            split_vertical: "Ctrl+Shift+O".to_string(),
            new_tab: "Ctrl+Shift+T".to_string(),
            close_pane: "Ctrl+Shift+W".to_string(),
            focus_next_pane: "Ctrl+Tab".to_string(),
            focus_prev_pane: "Ctrl+Shift+Tab".to_string(),
            open_preferences: "Ctrl+,".to_string(),
            close_preferences: "Ctrl+Shift+W".to_string(),
            toggle_collapse_preferences: "Ctrl+Shift+M".to_string(),
            toggle_fullscreen: "F11".to_string(),
            zoom_in: "Ctrl+=".to_string(),
            zoom_out: "Ctrl+-".to_string(),
            zoom_reset: "Ctrl+0".to_string(),
            toggle_move_mode: default_toggle_move_mode(),
            toggle_resize_mode: default_toggle_resize_mode(),
            toggle_broadcast_mode: default_toggle_broadcast_mode(),
        }
    }
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
    pub terminal_bg_opacity: f32,
    pub separator_width: f32,
    pub frame_border_width: f32,
    pub target_fps: u32,
    pub show_scrollbar: bool,
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
            terminal_bg_opacity: 1.0,
            separator_width: 2.0,
            frame_border_width: 5.0,
            target_fps: 30,
            show_scrollbar: true,
            font_path: default_debian_cjk_fallback(),
            ui_font_size: 13.0,
            terminal_font_size: 14.0,
            language: "auto".to_string(),
            custom_po_path: String::new(),
            shortcuts: ShortcutSettings::default(),
        }
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
        self.ui_font_size = self.ui_font_size.clamp(8.0, 32.0);
        self.terminal_font_size = self.terminal_font_size.clamp(8.0, 48.0);
        if self.language.trim().is_empty() {
            self.language = "auto".to_string();
        }
    }

    pub fn load(json_override: Option<&str>) -> Self {
        let primary = config_path();
        let raw_content = fs::read_to_string(&primary).or_else(|_| {
            let legacy = legacy_config_path();
            fs::read_to_string(&legacy)
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

/// Default Debian CJK monospace font search order (Section 4.3.1).
pub fn default_debian_cjk_fallback() -> String {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/opentype/noto/NotoSansMonoCJKjp-Regular.otf",
        "/usr/share/fonts/truetype/vlgothic/VL-Gothic-Regular.ttf",
        "/usr/share/fonts/truetype/vlgothic/VL-Gothic-regular.ttf",
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
        "/usr/share/fonts/truetype/ipafont-gothic/ipag.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
    ];
    for &path in CANDIDATES {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }
    CANDIDATES[0].to_string()
}

/// Validates TrueType / OpenType / TTC header magic and table bounds (Section 4.3.2).
pub fn is_valid_font_bytes(bytes: &[u8]) -> bool {
    if bytes.len() < 12 {
        return false;
    }
    let magic = &bytes[0..4];
    if magic == b"ttcf" {
        if bytes.len() < 16 {
            return false;
        }
        let num_fonts = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        if num_fonts == 0 || bytes.len() < 12 + num_fonts * 4 {
            return false;
        }
        let first_offset =
            u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        return first_offset + 12 <= bytes.len();
    }

    let valid_sfnt = magic == [0x00, 0x01, 0x00, 0x00]
        || magic == b"OTTO"
        || magic == b"true"
        || magic == b"typ1";
    if !valid_sfnt {
        return false;
    }
    let num_tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    num_tables > 0 && bytes.len() >= 12 + num_tables * 16
}

static FONT_FAMILY_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
static VALID_FONT_CACHE: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();
static SYSTEM_FONTS_CACHE: OnceLock<Mutex<Option<Vec<PathBuf>>>> = OnceLock::new();

/// Fast check whether a font file on disk has a valid header and size <= 35MB without reading the full file.
pub fn is_valid_font_file(path: &Path) -> bool {
    let cache_mutex = VALID_FONT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache_mutex.lock()
        && let Some(&cached) = guard.get(path)
    {
        return cached;
    }

    let valid = (|| {
        let meta = fs::metadata(path).ok()?;
        let file_len = meta.len() as usize;
        if file_len < 12 || file_len > 35 * 1024 * 1024 {
            return Some(false);
        }
        let mut file = fs::File::open(path).ok()?;
        let mut header = vec![0u8; file_len.min(4096)];
        file.read_exact(&mut header).ok()?;
        let magic = &header[0..4];
        if magic == b"ttcf" {
            if header.len() < 16 {
                return Some(false);
            }
            let num_fonts =
                u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
            if num_fonts == 0 || file_len < 12 + num_fonts * 4 {
                return Some(false);
            }
            let first_offset =
                u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as usize;
            return Some(first_offset + 12 <= file_len);
        }
        let valid_sfnt = magic == [0x00, 0x01, 0x00, 0x00]
            || magic == b"OTTO"
            || magic == b"true"
            || magic == b"typ1";
        if !valid_sfnt {
            return Some(false);
        }
        let num_tables = u16::from_be_bytes([header[4], header[5]]) as usize;
        Some(num_tables > 0 && file_len >= 12 + num_tables * 16)
    })()
    .unwrap_or(false);

    if let Ok(mut guard) = cache_mutex.lock() {
        guard.insert(path.to_path_buf(), valid);
    }
    valid
}

fn extract_font_family_from_file_fast(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    let file_len = meta.len() as usize;
    if file_len < 12 || file_len > 35 * 1024 * 1024 {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let mut header = vec![0u8; file_len.min(4096)];
    file.read_exact(&mut header).ok()?;

    let sfnt_offset = if &header[0..4] == b"ttcf" {
        if header.len() < 16 {
            return None;
        }
        u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as usize
    } else {
        0
    };

    // If the first sfnt header is beyond our initial 4KB buffer, seek to it
    let sfnt_buf = if sfnt_offset + 12 + 64 * 16 <= header.len() {
        header[sfnt_offset..].to_vec()
    } else if sfnt_offset + 12 <= file_len {
        file.seek(SeekFrom::Start(sfnt_offset as u64)).ok()?;
        let to_read = (file_len - sfnt_offset).min(4096);
        let mut buf = vec![0u8; to_read];
        file.read_exact(&mut buf).ok()?;
        buf
    } else {
        return None;
    };

    if sfnt_buf.len() < 12 {
        return None;
    }
    let magic = &sfnt_buf[0..4];
    let valid_sfnt = magic == [0x00, 0x01, 0x00, 0x00]
        || magic == b"OTTO"
        || magic == b"true"
        || magic == b"typ1";
    if !valid_sfnt {
        return None;
    }

    let num_tables = u16::from_be_bytes([sfnt_buf[4], sfnt_buf[5]]) as usize;
    let mut name_table_offset = None;
    let mut name_table_len = 0usize;

    for i in 0..num_tables {
        let rec = 12 + i * 16;
        if rec + 16 > sfnt_buf.len() {
            break;
        }
        if &sfnt_buf[rec..rec + 4] == b"name" {
            let offset = u32::from_be_bytes([
                sfnt_buf[rec + 8],
                sfnt_buf[rec + 9],
                sfnt_buf[rec + 10],
                sfnt_buf[rec + 11],
            ]) as usize;
            let length = u32::from_be_bytes([
                sfnt_buf[rec + 12],
                sfnt_buf[rec + 13],
                sfnt_buf[rec + 14],
                sfnt_buf[rec + 15],
            ]) as usize;
            name_table_offset = Some(offset);
            name_table_len = length;
            break;
        }
    }

    let table_start = name_table_offset?;
    if table_start + 6 > file_len || name_table_len < 6 {
        return None;
    }
    let read_len = name_table_len.min(file_len - table_start).min(128 * 1024);
    file.seek(SeekFrom::Start(table_start as u64)).ok()?;
    let mut name_buf = vec![0u8; read_len];
    file.read_exact(&mut name_buf).ok()?;
    parse_name_table_slice(&name_buf)
}

/// Extracts the OpenType/TrueType font family name from a font file path,
/// falling back to a heuristic based on the filename stem.
pub fn extract_font_family_from_path(path_str: &str) -> Option<String> {
    let trimmed = path_str.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = Path::new(trimmed);
    if !path.is_absolute() && !trimmed.contains('/') {
        return Some(trimmed.to_string());
    }

    let cache_mutex = FONT_FAMILY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache_mutex.lock()
        && let Some(cached) = guard.get(trimmed)
    {
        return cached.clone();
    }

    let resolved = if let Some(family) = extract_font_family_from_file_fast(path) {
        Some(family)
    } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        let base = stem
            .split('-')
            .next()
            .unwrap_or(stem)
            .replace('_', " ");
        let lower = base.to_ascii_lowercase();
        if lower.contains("notosansmonocjkjp") {
            Some("Noto Sans Mono CJK JP".to_string())
        } else if lower.contains("notosanscjk") {
            Some("Noto Sans CJK JP".to_string())
        } else if lower.contains("vlgothic") || lower == "vl" {
            Some("VL Gothic".to_string())
        } else if lower == "ipag" {
            Some("IPAGothic".to_string())
        } else if lower.contains("dejavusansmono") {
            Some("DejaVu Sans Mono".to_string())
        } else {
            Some(base)
        }
    } else {
        None
    };

    if let Ok(mut guard) = cache_mutex.lock() {
        guard.insert(trimmed.to_string(), resolved.clone());
    }
    resolved
}

pub fn extract_font_family_from_bytes(bytes: &[u8]) -> Option<String> {
    if !is_valid_font_bytes(bytes) {
        return None;
    }
    let sfnt_offset = if &bytes[0..4] == b"ttcf" {
        u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize
    } else {
        0
    };
    if sfnt_offset + 12 > bytes.len() {
        return None;
    }
    let num_tables = u16::from_be_bytes([bytes[sfnt_offset + 4], bytes[sfnt_offset + 5]]) as usize;
    let mut name_table_offset = None;
    let mut name_table_len = 0usize;

    for i in 0..num_tables {
        let rec = sfnt_offset + 12 + i * 16;
        if rec + 16 > bytes.len() {
            break;
        }
        if &bytes[rec..rec + 4] == b"name" {
            let offset =
                u32::from_be_bytes([bytes[rec + 8], bytes[rec + 9], bytes[rec + 10], bytes[rec + 11]])
                    as usize;
            let length = u32::from_be_bytes([
                bytes[rec + 12],
                bytes[rec + 13],
                bytes[rec + 14],
                bytes[rec + 15],
            ]) as usize;
            name_table_offset = Some(offset);
            name_table_len = length;
            break;
        }
    }

    let table_start = name_table_offset?;
    if table_start + 6 > bytes.len() || name_table_len < 6 {
        return None;
    }
    let table_end = (table_start + name_table_len).min(bytes.len());
    parse_name_table_slice(&bytes[table_start..table_end])
}

fn parse_name_table_slice(name_table: &[u8]) -> Option<String> {
    if name_table.len() < 6 {
        return None;
    }
    let count = u16::from_be_bytes([name_table[2], name_table[3]]) as usize;
    let string_offset = u16::from_be_bytes([name_table[4], name_table[5]]) as usize;
    let storage_base = string_offset;

    let mut preferred_family: Option<String> = None;
    let mut standard_family: Option<String> = None;

    for i in 0..count {
        let rec = 6 + i * 12;
        if rec + 12 > name_table.len() {
            break;
        }
        let platform_id = u16::from_be_bytes([name_table[rec], name_table[rec + 1]]);
        let lang_id = u16::from_be_bytes([name_table[rec + 4], name_table[rec + 5]]);
        let name_id = u16::from_be_bytes([name_table[rec + 6], name_table[rec + 7]]);
        let length = u16::from_be_bytes([name_table[rec + 8], name_table[rec + 9]]) as usize;
        let offset = u16::from_be_bytes([name_table[rec + 10], name_table[rec + 11]]) as usize;

        if name_id != 1 && name_id != 16 {
            continue;
        }
        let str_start = storage_base + offset;
        let str_end = str_start + length;
        if str_end > name_table.len() || length == 0 {
            continue;
        }
        let raw = &name_table[str_start..str_end];
        let decoded = if platform_id == 3 || platform_id == 0 {
            // UTF-16BE
            if raw.len() % 2 != 0 {
                continue;
            }
            let u16s: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16(&u16s).ok()
        } else if platform_id == 1 {
            // Mac Roman / ASCII
            String::from_utf8(raw.to_vec()).ok()
        } else {
            None
        };

        if let Some(s) = decoded {
            let clean = s.trim_matches('\0').trim().to_string();
            if clean.is_empty() || clean.starts_with('.') {
                continue;
            }
            let is_english = (platform_id == 3 && lang_id == 0x0409)
                || (platform_id == 1 && lang_id == 0)
                || platform_id == 0;
            if name_id == 16 {
                if preferred_family.is_none() || is_english {
                    preferred_family = Some(clean);
                }
            } else if name_id == 1 {
                if standard_family.is_none() || is_english {
                    standard_family = Some(clean);
                }
            }
        }
    }

    preferred_family.or(standard_family)
}

/// Computes font sort priority (`0..=7`) according to Section 4.3.3.
pub fn font_sort_priority(path: &Path) -> u8 {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    let is_cjk = lower.contains("cjk")
        || lower.contains("jp")
        || lower.contains("japanese")
        || lower.contains("vlgothic")
        || lower.contains("ipa")
        || lower.contains("takao")
        || lower.contains("migmix")
        || lower.contains("migu")
        || lower.contains("ume")
        || lower.contains("sazanami")
        || lower.contains("kochi")
        || lower.contains("harano")
        || lower.contains("bizud");
    let is_mono = lower.contains("mono")
        || lower.contains("gothic")
        || lower.contains("ipag")
        || lower.contains("courier")
        || lower.contains("consol")
        || lower.contains("hack")
        || lower.contains("firacode")
        || lower.contains("jetbrains")
        || lower.contains("inconsolata")
        || lower.contains("menlo");
    let is_regular = lower.contains("regular")
        || (!lower.contains("bold")
            && !lower.contains("italic")
            && !lower.contains("oblique")
            && !lower.contains("light")
            && !lower.contains("thin")
            && !lower.contains("black")
            && !lower.contains("medium")
            && !lower.contains("condensed"));

    if is_cjk && is_mono {
        if is_regular { 0 } else { 1 }
    } else if is_cjk {
        if is_regular { 2 } else { 3 }
    } else if is_mono {
        if is_regular { 4 } else { 5 }
    } else if lower.contains("dejavu")
        || lower.contains("liberation")
        || lower.contains("roboto")
        || lower.contains("lato")
        || lower.contains("freesans")
        || lower.contains("freeserif")
        || lower.contains("notosans")
        || lower.contains("notoserif")
    {
        6
    } else {
        7
    }
}

pub fn is_font_cjk(path: &Path) -> bool {
    matches!(font_sort_priority(path), 0..=3)
}

pub fn is_font_monospace(path: &Path) -> bool {
    matches!(font_sort_priority(path), 0 | 1 | 4 | 5)
}

/// Returns cached system fonts, or scans if not yet cached or `force_rescan` is true.
pub fn cached_system_fonts(force_rescan: bool) -> Vec<PathBuf> {
    let cache_mutex = SYSTEM_FONTS_CACHE.get_or_init(|| Mutex::new(None));
    if !force_rescan
        && let Ok(guard) = cache_mutex.lock()
        && let Some(ref list) = *guard
    {
        return list.clone();
    }
    let scanned = discover_system_fonts();
    if let Ok(mut guard) = cache_mutex.lock() {
        *guard = Some(scanned.clone());
    }
    scanned
}

/// Scans standard Linux font directories and returns sorted font paths (Section 4.3.3).
pub fn discover_system_fonts() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        let h = PathBuf::from(home);
        roots.push(h.join(".local/share/fonts"));
        roots.push(h.join(".fonts"));
    }

    let mut files = Vec::new();
    for root in roots {
        collect_font_files_recursive(&root, &mut files);
    }

    files.sort_by(|a, b| {
        let pa = font_sort_priority(a);
        let pb = font_sort_priority(b);
        pa.cmp(&pb).then_with(|| a.cmp(b))
    });
    files.dedup();
    files
}

fn collect_font_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_font_files_recursive(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_ascii_lowercase();
            if ext_lower == "ttf" || ext_lower == "otf" || ext_lower == "ttc" {
                out.push(path);
            }
        }
    }
}

/// Splits a shortcut setting string into individual bindings (Section 4.4).
/// Supports `/`, `|`, and `,` delimiters while keeping `"Ctrl+,"` intact.
pub fn split_shortcut_bindings(spec: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c == '/' || c == '|' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                result.push(trimmed);
            }
            current.clear();
        } else if c == ',' {
            let prev_non_space = current.trim_end().chars().last();
            if prev_non_space == Some('+') || current.trim().is_empty() {
                current.push(c);
            } else {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
        } else {
            current.push(c);
        }
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBinding {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
    pub key: String,
}

pub fn parse_single_binding(binding: &str) -> Option<ParsedBinding> {
    let s = binding.trim();
    if s.is_empty() {
        return None;
    }

    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut super_key = false;
    let mut key_part: Option<String> = None;

    // Handle trailing `+` or `,` after `+` (e.g. "Ctrl+=", "Ctrl++", "Ctrl+,")
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch == '+' {
            if cur.trim().is_empty() {
                cur.push('+');
            } else {
                tokens.push(cur.trim().to_string());
                cur.clear();
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        tokens.push(cur.trim().to_string());
    } else if s.ends_with('+') {
        tokens.push("+".to_string());
    }

    for tok in tokens {
        let lower = tok.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" | "ctl" => ctrl = true,
            "shift" | "shft" => shift = true,
            "alt" | "option" | "opt" => alt = true,
            "super" | "win" | "windows" | "meta" | "cmd" | "command" | "logo" => super_key = true,
            _ => {
                key_part = Some(canonicalize_key_name(&tok));
            }
        }
    }

    let key = key_part?;
    if key.is_empty() {
        return None;
    }
    Some(ParsedBinding {
        ctrl,
        shift,
        alt,
        super_key,
        key,
    })
}

pub fn canonicalize_key_name(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "=" | "+" | "plus" | "equal" | "equals" | "kp_add" | "kp_equal" => "=".to_string(),
        "-" | "minus" | "hyphen" | "underscore" | "kp_subtract" => "-".to_string(),
        "," | "comma" => ",".to_string(),
        "." | "period" | "dot" => ".".to_string(),
        "/" | "slash" => "/".to_string(),
        "\\" | "backslash" => "\\".to_string(),
        "[" | "bracketleft" => "[".to_string(),
        "]" | "bracketright" => "]".to_string(),
        ";" | "semicolon" => ";".to_string(),
        "'" | "apostrophe" | "quote" => "'".to_string(),
        "`" | "grave" | "backtick" | "quoteleft" => "`".to_string(),
        "esc" | "escape" => "escape".to_string(),
        "ret" | "return" | "enter" | "kp_enter" => "enter".to_string(),
        "tab" | "iso_left_tab" | "kp_tab" => "tab".to_string(),
        "space" | " " => "space".to_string(),
        "bs" | "backspace" => "backspace".to_string(),
        "del" | "delete" => "delete".to_string(),
        "ins" | "insert" => "insert".to_string(),
        "pgup" | "pageup" | "page_up" | "prior" => "pageup".to_string(),
        "pgdn" | "pagedown" | "page_down" | "next" => "pagedown".to_string(),
        "arrowup" | "up" => "up".to_string(),
        "arrowdown" | "down" => "down".to_string(),
        "arrowleft" | "left" => "left".to_string(),
        "arrowright" | "right" => "right".to_string(),
        other => other.to_string(),
    }
}

pub fn matches_shortcut(
    spec: &str,
    key_name: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
    super_key: bool,
) -> bool {
    let target_key = canonicalize_key_name(key_name);
    if target_key.is_empty() {
        return false;
    }
    for single in split_shortcut_bindings(spec) {
        if let Some(parsed) = parse_single_binding(&single) {
            if parsed.key == target_key
                && parsed.ctrl == ctrl
                && parsed.alt == alt
                && parsed.super_key == super_key
            {
                // For `=` / `+`, keyboard layouts vary on whether Shift is required to produce `+`
                if parsed.key == "=" && (parsed.shift == shift || !parsed.shift) {
                    return true;
                }
                if parsed.shift == shift {
                    return true;
                }
            }
        }
    }
    false
}

pub fn gdk_key_to_canonical_name(key: gdk::Key) -> Option<String> {
    match key {
        gdk::Key::Control_L
        | gdk::Key::Control_R
        | gdk::Key::Shift_L
        | gdk::Key::Shift_R
        | gdk::Key::Alt_L
        | gdk::Key::Alt_R
        | gdk::Key::Meta_L
        | gdk::Key::Meta_R
        | gdk::Key::Super_L
        | gdk::Key::Super_R
        | gdk::Key::Hyper_L
        | gdk::Key::Hyper_R
        | gdk::Key::Caps_Lock
        | gdk::Key::Shift_Lock => return None,
        gdk::Key::ISO_Left_Tab | gdk::Key::Tab | gdk::Key::KP_Tab => {
            return Some("tab".to_string());
        }
        gdk::Key::Return | gdk::Key::KP_Enter => return Some("enter".to_string()),
        gdk::Key::Escape => return Some("escape".to_string()),
        gdk::Key::BackSpace => return Some("backspace".to_string()),
        gdk::Key::Delete | gdk::Key::KP_Delete => return Some("delete".to_string()),
        gdk::Key::Insert | gdk::Key::KP_Insert => return Some("insert".to_string()),
        gdk::Key::Home | gdk::Key::KP_Home => return Some("home".to_string()),
        gdk::Key::End | gdk::Key::KP_End => return Some("end".to_string()),
        gdk::Key::Page_Up | gdk::Key::KP_Page_Up => return Some("pageup".to_string()),
        gdk::Key::Page_Down | gdk::Key::KP_Page_Down => return Some("pagedown".to_string()),
        gdk::Key::Up | gdk::Key::KP_Up => return Some("up".to_string()),
        gdk::Key::Down | gdk::Key::KP_Down => return Some("down".to_string()),
        gdk::Key::Left | gdk::Key::KP_Left => return Some("left".to_string()),
        gdk::Key::Right | gdk::Key::KP_Right => return Some("right".to_string()),
        gdk::Key::space => return Some("space".to_string()),
        gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add | gdk::Key::KP_Equal => {
            return Some("=".to_string());
        }
        gdk::Key::minus | gdk::Key::underscore | gdk::Key::KP_Subtract => {
            return Some("-".to_string());
        }
        gdk::Key::comma => return Some(",".to_string()),
        gdk::Key::period | gdk::Key::KP_Decimal => return Some(".".to_string()),
        gdk::Key::slash | gdk::Key::KP_Divide => return Some("/".to_string()),
        gdk::Key::backslash => return Some("\\".to_string()),
        _ => {}
    }

    if let Some(ch) = key.to_unicode() {
        if !ch.is_control() {
            return Some(canonicalize_key_name(&ch.to_string()));
        }
    }
    let name = key.name()?;
    Some(canonicalize_key_name(name.as_str()))
}

pub fn matches_gdk_shortcut(spec: &str, key: gdk::Key, mods: gdk::ModifierType) -> bool {
    let Some(key_name) = gdk_key_to_canonical_name(key) else {
        return false;
    };
    let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
    let shift =
        mods.contains(gdk::ModifierType::SHIFT_MASK) || key == gdk::Key::ISO_Left_Tab;
    let alt = mods.contains(gdk::ModifierType::ALT_MASK);
    let super_key =
        mods.contains(gdk::ModifierType::SUPER_MASK) || mods.contains(gdk::ModifierType::META_MASK);

    matches_shortcut(spec, &key_name, ctrl, shift, alt, super_key)
}

/// Formats a pressed GDK key + modifiers into `"Super+Ctrl+Shift+Alt+Key"` for shortcut recording (Section 11.2.3).
pub fn format_recorded_shortcut(key: gdk::Key, mods: gdk::ModifierType) -> Option<String> {
    let key_name = gdk_key_to_canonical_name(key)?;
    let mut parts = Vec::new();
    if mods.contains(gdk::ModifierType::SUPER_MASK) || mods.contains(gdk::ModifierType::META_MASK)
    {
        parts.push("Super".to_string());
    }
    if mods.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl".to_string());
    }
    if mods.contains(gdk::ModifierType::SHIFT_MASK) || key == gdk::Key::ISO_Left_Tab {
        parts.push("Shift".to_string());
    }
    if mods.contains(gdk::ModifierType::ALT_MASK) {
        parts.push("Alt".to_string());
    }

    let display_key = if key_name.len() == 1 {
        key_name.to_ascii_uppercase()
    } else {
        let mut chars = key_name.chars();
        match chars.next() {
            Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            None => key_name,
        }
    };
    parts.push(display_key);
    Some(parts.join("+"))
}

#[cfg(test)]
#[path = "test/prefs_ui_tests.rs"]
mod prefs_ui_tests;
