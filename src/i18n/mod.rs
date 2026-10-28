// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

pub const EMBEDDED_EN_PO: &str = include_str!("../../po/en.po");
pub const EMBEDDED_JA_PO: &str = include_str!("../../po/ja.po");

#[derive(Debug, Clone, Default)]
pub struct PoCatalog {
    pub locale: String,
    pub headers: HashMap<String, String>,
    pub translations: HashMap<String, String>,
    pub plurals: HashMap<String, Vec<String>>,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseSection {
    None,
    Msgctxt,
    Msgid,
    MsgidPlural,
    Msgstr,
    MsgstrPlural(usize),
}

impl PoCatalog {
    pub fn new(locale: impl Into<String>) -> Self {
        Self {
            locale: locale.into(),
            headers: HashMap::new(),
            translations: HashMap::new(),
            plurals: HashMap::new(),
        }
    }

    pub fn parse(content: &str, default_locale: &str) -> Self {
        let mut catalog = Self::new(default_locale);

        let mut current_msgctxt: Option<String> = None;
        let mut current_msgid: Option<String> = None;
        let mut current_msgid_plural: Option<String> = None;
        let mut current_msgstr: Option<String> = None;
        let mut current_msgstr_plurals: HashMap<usize, String> = HashMap::new();
        let mut is_fuzzy = false;
        let mut section = ParseSection::None;

        let flush_entry = |cat: &mut PoCatalog,
                           msgctxt: &mut Option<String>,
                           msgid: &mut Option<String>,
                           msgid_plural: &mut Option<String>,
                           msgstr: &mut Option<String>,
                           msgstr_plurals: &mut HashMap<usize, String>,
                           fuzzy: &mut bool| {
            if let Some(id) = msgid.take() {
                let ctx = msgctxt.take();
                let plural_id = msgid_plural.take();
                let s = msgstr.take().unwrap_or_default();
                let mut p_map = std::mem::take(msgstr_plurals);
                let was_fuzzy = *fuzzy;
                *fuzzy = false;

                if id.is_empty() && ctx.is_none() {
                    // Parse header metadata lines
                    for line in s.lines() {
                        if let Some((k, v)) = line.split_once(':') {
                            let key = k.trim().to_string();
                            let val = v.trim().to_string();
                            if key.eq_ignore_ascii_case("Language") && !val.is_empty() {
                                cat.locale = val.clone();
                            }
                            cat.headers.insert(key, val);
                        }
                    }
                    return;
                }

                if was_fuzzy {
                    return;
                }

                let key = match ctx {
                    Some(c) => format!("{}\u{0004}{}", c, id),
                    None => id,
                };

                if plural_id.is_some() || !p_map.is_empty() {
                    let max_idx = p_map.keys().copied().max().unwrap_or(0);
                    let mut vec = Vec::with_capacity(max_idx + 1);
                    for i in 0..=max_idx {
                        vec.push(p_map.remove(&i).unwrap_or_default());
                    }
                    if vec.iter().any(|v| !v.is_empty()) {
                        if let Some(first) = vec.first() {
                            if !first.is_empty() {
                                cat.translations.insert(key.clone(), first.clone());
                            }
                        }
                        cat.plurals.insert(key, vec);
                    }
                } else if !s.is_empty() {
                    cat.translations.insert(key, s);
                }
            } else {
                *msgctxt = None;
                *msgid_plural = None;
                *msgstr = None;
                msgstr_plurals.clear();
                *fuzzy = false;
            }
        };

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                flush_entry(
                    &mut catalog,
                    &mut current_msgctxt,
                    &mut current_msgid,
                    &mut current_msgid_plural,
                    &mut current_msgstr,
                    &mut current_msgstr_plurals,
                    &mut is_fuzzy,
                );
                section = ParseSection::None;
                continue;
            }

            if line.starts_with('#') {
                if let Some(flags) = line.strip_prefix("#,") {
                    for flag in flags.split(',') {
                        if flag.trim() == "fuzzy" {
                            is_fuzzy = true;
                        }
                    }
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("msgctxt ") {
                if matches!(
                    section,
                    ParseSection::Msgstr | ParseSection::MsgstrPlural(_)
                ) {
                    flush_entry(
                        &mut catalog,
                        &mut current_msgctxt,
                        &mut current_msgid,
                        &mut current_msgid_plural,
                        &mut current_msgstr,
                        &mut current_msgstr_plurals,
                        &mut is_fuzzy,
                    );
                }
                current_msgctxt = Some(unquote_po_string(rest.trim()));
                section = ParseSection::Msgctxt;
            } else if let Some(rest) = line.strip_prefix("msgid_plural ") {
                current_msgid_plural = Some(unquote_po_string(rest.trim()));
                section = ParseSection::MsgidPlural;
            } else if let Some(rest) = line.strip_prefix("msgid ") {
                if matches!(
                    section,
                    ParseSection::Msgstr | ParseSection::MsgstrPlural(_)
                ) {
                    flush_entry(
                        &mut catalog,
                        &mut current_msgctxt,
                        &mut current_msgid,
                        &mut current_msgid_plural,
                        &mut current_msgstr,
                        &mut current_msgstr_plurals,
                        &mut is_fuzzy,
                    );
                }
                current_msgid = Some(unquote_po_string(rest.trim()));
                section = ParseSection::Msgid;
            } else if let Some(rest) = line.strip_prefix("msgstr[") {
                if let Some((idx_str, after_bracket)) = rest.split_once(']') {
                    if let Ok(idx) = idx_str.trim().parse::<usize>() {
                        let val = unquote_po_string(after_bracket.trim());
                        current_msgstr_plurals.insert(idx, val);
                        section = ParseSection::MsgstrPlural(idx);
                    }
                }
            } else if let Some(rest) = line.strip_prefix("msgstr ") {
                current_msgstr = Some(unquote_po_string(rest.trim()));
                section = ParseSection::Msgstr;
            } else if line.starts_with('"') && line.ends_with('"') {
                let piece = unquote_po_string(line);
                match section {
                    ParseSection::Msgctxt => {
                        if let Some(ref mut s) = current_msgctxt {
                            s.push_str(&piece);
                        }
                    }
                    ParseSection::Msgid => {
                        if let Some(ref mut s) = current_msgid {
                            s.push_str(&piece);
                        }
                    }
                    ParseSection::MsgidPlural => {
                        if let Some(ref mut s) = current_msgid_plural {
                            s.push_str(&piece);
                        }
                    }
                    ParseSection::Msgstr => {
                        if let Some(ref mut s) = current_msgstr {
                            s.push_str(&piece);
                        }
                    }
                    ParseSection::MsgstrPlural(idx) => {
                        current_msgstr_plurals.entry(idx).or_default().push_str(&piece);
                    }
                    ParseSection::None => {}
                }
            }
        }

        flush_entry(
            &mut catalog,
            &mut current_msgctxt,
            &mut current_msgid,
            &mut current_msgid_plural,
            &mut current_msgstr,
            &mut current_msgstr_plurals,
            &mut is_fuzzy,
        );

        catalog
    }

    pub fn get<'a>(&'a self, msgid: &'a str) -> &'a str {
        self.translations
            .get(msgid)
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(msgid)
    }

    pub fn get_ctx<'a>(&'a self, msgctxt: &str, msgid: &'a str) -> &'a str {
        let key = format!("{}\u{0004}{}", msgctxt, msgid);
        self.translations
            .get(&key)
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(msgid)
    }

    pub fn get_plural<'a>(&'a self, msgid: &'a str, msgid_plural: &'a str, n: usize) -> &'a str {
        if let Some(forms) = self.plurals.get(msgid) {
            let idx = if forms.len() == 1 {
                0
            } else if n == 1 {
                0
            } else {
                1.min(forms.len().saturating_sub(1))
            };
            if let Some(found) = forms.get(idx) {
                if !found.is_empty() {
                    return found.as_str();
                }
            }
        }
        if n == 1 {
            msgid
        } else {
            msgid_plural
        }
    }

    pub fn entry_count(&self) -> usize {
        self.translations.len()
    }
}

pub fn unquote_po_string(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn detect_system_language() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            if let Some(code) = extract_lang_code(&val) {
                return code;
            }
        }
    }
    "en".to_string()
}

pub fn extract_lang_code(locale_str: &str) -> Option<String> {
    let trimmed = locale_str.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("C")
        || trimmed.eq_ignore_ascii_case("POSIX")
        || trimmed.starts_with("C.")
    {
        return None;
    }
    let without_encoding = trimmed.split('.').next().unwrap_or(trimmed);
    let without_modifier = without_encoding.split('@').next().unwrap_or(without_encoding);
    let primary = without_modifier
        .split('_')
        .next()
        .unwrap_or(without_modifier)
        .trim()
        .to_ascii_lowercase();
    if primary.is_empty() {
        None
    } else {
        Some(primary)
    }
}

pub fn po_search_directories() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("./po")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            dirs.push(exe_dir.join("po"));
            if let Some(parent_dir) = exe_dir.parent() {
                dirs.push(parent_dir.join("po"));
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        dirs.push(home_path.join(".config/termin/po"));
        dirs.push(home_path.join(".config/fwer/po"));
    }
    dirs
}

pub fn discover_po_files() -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for dir in po_search_directories() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("po") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let code = stem.to_string();
                        if !found.iter().any(|(c, _)| c == &code) {
                            found.push((code, path));
                        }
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

pub fn resolve_catalog(language: &str, custom_po_path: &str) -> PoCatalog {
    let custom_trimmed = custom_po_path.trim();
    if !custom_trimmed.is_empty() {
        if let Ok(content) = fs::read_to_string(Path::new(custom_trimmed)) {
            return PoCatalog::parse(&content, "custom");
        }
    }

    let resolved_lang = if language.trim().is_empty() || language.trim().eq_ignore_ascii_case("auto")
    {
        detect_system_language()
    } else {
        language.trim().to_ascii_lowercase()
    };

    for dir in po_search_directories() {
        let candidate = dir.join(format!("{}.po", resolved_lang));
        if let Ok(content) = fs::read_to_string(&candidate) {
            return PoCatalog::parse(&content, &resolved_lang);
        }
    }

    if resolved_lang.starts_with("ja") {
        PoCatalog::parse(EMBEDDED_JA_PO, "ja")
    } else {
        PoCatalog::parse(EMBEDDED_EN_PO, "en")
    }
}

static ACTIVE_CATALOG: OnceLock<RwLock<PoCatalog>> = OnceLock::new();

fn catalog_lock() -> &'static RwLock<PoCatalog> {
    ACTIVE_CATALOG.get_or_init(|| RwLock::new(resolve_catalog("auto", "")))
}

pub fn load_catalog_for_language(language: &str, custom_po_path: &str) {
    let new_cat = resolve_catalog(language, custom_po_path);
    if let Ok(mut guard) = catalog_lock().write() {
        *guard = new_cat;
    }
}

pub fn tr(msgid: &str) -> String {
    if let Ok(guard) = catalog_lock().read() {
        guard.get(msgid).to_string()
    } else {
        msgid.to_string()
    }
}

pub fn tr_ctx(msgctxt: &str, msgid: &str) -> String {
    if let Ok(guard) = catalog_lock().read() {
        guard.get_ctx(msgctxt, msgid).to_string()
    } else {
        msgid.to_string()
    }
}

pub fn trn(msgid: &str, msgid_plural: &str, n: usize) -> String {
    if let Ok(guard) = catalog_lock().read() {
        guard.get_plural(msgid, msgid_plural, n).to_string()
    } else if n == 1 {
        msgid.to_string()
    } else {
        msgid_plural.to_string()
    }
}

pub fn active_locale() -> String {
    if let Ok(guard) = catalog_lock().read() {
        guard.locale.clone()
    } else {
        "en".to_string()
    }
}

pub fn active_entry_count() -> usize {
    if let Ok(guard) = catalog_lock().read() {
        guard.entry_count()
    } else {
        0
    }
}

#[cfg(test)]
#[path = "test/i18n_tests.rs"]
mod i18n_tests;
