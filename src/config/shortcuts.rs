// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use gtk4::gdk;
use serde::{Deserialize, Serialize};

macro_rules! define_shortcuts {
    // 1. エントリポイント
    (
        $($field:ident => {
                label: $label:literal,
                default: $default:literal$(, action: $action:ident)?$(,)?
            }
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(default)]
        pub struct ShortcutSettings {
            $(
                pub $field: String,
            )*
        }

        impl Default for ShortcutSettings {
            fn default() -> Self {
                Self {
                    $(
                        $field:$default.to_string(),
                    )*
                }
            }
        }

        // Automatically generates the PaneShortcutAction
        // enum from items that have a specified action
        define_shortcuts!(@collect_enum [] $($field $(:$action)?, )*);

        impl ShortcutSettings {
            /// Returns a list of (slot_key, label, current_val, default_val) in definition order for
            /// constructing the preferences UI rows.
            pub fn slots(&self) -> Vec<(&'static str, &'static str, &str, &'static str)> {
                vec![
                    $(
                        (stringify!($field),$label, self.$field.as_str(),$default),
                    )*
                ]
            }

            /// Get the current configuration string from the slot_key
            pub fn get_slot(&self, slot_key: &str) -> String {
                match slot_key {
                    $(
                        stringify!($field) => self.$field.clone(),
                    )*
                    _ => String::new(),
                }
            }

            /// Updates the field matched with the slot_key (append supported).
            pub fn update_slot(&mut self, slot_key: &str, new_val: &str, append: bool) {
                let update = |field: &mut String| {
                    if append && !is_shortcut_disabled(field) {
                        let mut list = split_shortcut_bindings(field);
                        if !is_shortcut_disabled_keyword(new_val)
                            && !list.iter().any(|x| x.eq_ignore_ascii_case(new_val))
                        {
                            list.push(new_val.to_string());
                        }
                        *field = list.join(" / ");
                    } else {
                        *field = new_val.to_string();
                    }
                };

                match slot_key {
                    $(
                        stringify!($field) => update(&mut self.$field),
                    )*
                    _ => {}
                }
            }

            /// 押されたキーに一致する PaneShortcutAction を返す（pane.rs の check_sc 代替）
            pub fn match_pane_shortcut(
                &self,
                key: gdk::Key,
                mods: gdk::ModifierType,
            ) -> Option<PaneShortcutAction> {
                $(
                    define_shortcuts!(@check_pane self, key, mods, $field $(,$action)?);
                )*
                None
            }
        }
    };

    // 2. PaneShortcutAction 生成用 TT-muncher
    (@collect_enum [ $($acc:ident,)* ]) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum PaneShortcutAction {
            $($acc, )*
        }
    };
    (@collect_enum [ $($acc:ident,)* ] $field:ident :$action:ident, $($rest:tt)*) => {
        define_shortcuts!(@collect_enum [ $($acc,)*$action, ] $($rest)*);
    };
    (@collect_enum [ $($acc:ident,)* ]$field:ident, $($rest:tt)*) => {
        define_shortcuts!(@collect_enum [ $($acc,)* ] $($rest)*);
    };

    // 3. ショートカット一致判定展開用ヘルパー
    (@check_pane $self:ident, $key:ident,$mods:ident, $field:ident,$action:ident) => {
        if matches_gdk_shortcut(&$self.$field, $key,$mods) {
            return Some(PaneShortcutAction::$action);
        }
    };
    (@check_pane $self:ident,$key:ident, $mods:ident,$field:ident) => {};
}

// ショートカット定義リスト（今後ショートカットを増やす際はここに1行足すだけ）
define_shortcuts! {
    // Clipboard Operations
    copy => { label: "Copy", default: "Ctrl+Shift+C", action: Copy },
    paste => { label: "Paste", default: "Ctrl+Shift+V", action: Paste },

    // Pane & Tab Operations
    split_horizontal => { label: "Split Horizontally", default: "Ctrl+Shift+E", action: SplitHorizontal },
    split_vertical => { label: "Split Vertically", default: "Ctrl+Shift+O", action: SplitVertical },
    new_tab => { label: "New Tab", default: "Ctrl+Shift+T", action: NewTab },
    close_pane => { label: "Close Pane", default: "Ctrl+Shift+W", action: ClosePane },

    // Pane Navigation
    focus_next_pane => { label: "Focus Next Pane", default: "Ctrl+Tab", action: FocusNextPane },
    focus_prev_pane => { label: "Focus Prev Pane", default: "Ctrl+Shift+Tab", action: FocusPrevPane },

    // Settings Screen (& Resolving Key Conflicts with ClosePane / MoveMode)
    open_preferences => { label: "Open Preferences", default: "Ctrl+Shift+S", action: OpenPreferences },
    close_preferences => { label: "Close Preferences", default: "Ctrl+Shift+Q" },
    toggle_collapse_preferences => { label: "Toggle Collapse Preferences", default: "Ctrl+Shift+P" },

    // Screen Display & Zoom
    toggle_fullscreen => { label: "Toggle Fullscreen", default: "F11", action: ToggleFullscreen },
    zoom_in => { label: "Zoom In", default: "Super+=", action: ZoomIn },
    zoom_out => { label: "Zoom Out", default: "Super+-", action: ZoomOut },
    zoom_reset => { label: "Zoom Reset", default: "Super+0", action: ZoomReset },

    // Mode Switching
    toggle_move_mode => { label: "Pane Move Mode", default: "Ctrl+Shift+M", action: ToggleMoveMode },
    toggle_resize_mode => { label: "Split Resize Mode", default: "Ctrl+Shift+R", action: ToggleResizeMode },
    toggle_broadcast_mode => { label: "Broadcast Mode", default: "Ctrl+Shift+B", action: ToggleBroadcastMode },
    toggle_opacity_mode => { label: "Opacity Adjust Mode", default: "Ctrl+Shift+U", action: ToggleOpacityMode },
    toggle_scrollbar => { label: "Toggle Scrollbar", default: "Ctrl+Shift+L", action: ToggleScrollbar },
}

/// Returns true if a single shortcut token represents an explicitly disabled state.
pub fn is_shortcut_disabled_keyword(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "無効" {
        return true;
    }
    matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "disabled" | "disable" | "none" | "off"
    )
}

/// Returns true if a shortcut specification has no active bindings (disabled).
pub fn is_shortcut_disabled(spec: &str) -> bool {
    split_shortcut_bindings(spec).is_empty()
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
            if !is_shortcut_disabled_keyword(&trimmed) {
                result.push(trimmed);
            }
            current.clear();
        } else if c == ',' {
            let prev_non_space = current.trim_end().chars().last();
            if prev_non_space == Some('+') || current.trim().is_empty() {
                current.push(c);
            } else {
                let trimmed = current.trim().to_string();
                if !is_shortcut_disabled_keyword(&trimmed) {
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
    if !is_shortcut_disabled_keyword(&trimmed) {
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
    if is_shortcut_disabled_keyword(s) {
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
    let shift = mods.contains(gdk::ModifierType::SHIFT_MASK) || key == gdk::Key::ISO_Left_Tab;
    let alt = mods.contains(gdk::ModifierType::ALT_MASK);
    let super_key =
        mods.contains(gdk::ModifierType::SUPER_MASK) || mods.contains(gdk::ModifierType::META_MASK);

    matches_shortcut(spec, &key_name, ctrl, shift, alt, super_key)
}

/// Formats a pressed GDK key + modifiers into `"Super+Ctrl+Shift+Alt+Key"` for shortcut recording (Section 11.2.3).
pub fn format_recorded_shortcut(key: gdk::Key, mods: gdk::ModifierType) -> Option<String> {
    let key_name = gdk_key_to_canonical_name(key)?;
    let mut parts = Vec::new();
    if mods.contains(gdk::ModifierType::SUPER_MASK) || mods.contains(gdk::ModifierType::META_MASK) {
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
