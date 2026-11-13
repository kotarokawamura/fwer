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
use std::sync::{Mutex, OnceLock};

static FONT_FAMILY_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
static VALID_FONT_CACHE: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();
static SYSTEM_FONTS_CACHE: OnceLock<Mutex<Option<Vec<PathBuf>>>> = OnceLock::new();

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

fn is_valid_sfnt_magic(magic: &[u8]) -> bool {
    magic == [0x00, 0x01, 0x00, 0x00] || magic == b"OTTO" || magic == b"true" || magic == b"typ1"
}

/// Shared validator for TrueType / OpenType / TTC header bytes against total byte/file length.
fn check_font_header_bounds(header: &[u8], total_len: usize) -> bool {
    if header.len() < 12 || total_len < 12 {
        return false;
    }
    let magic = &header[0..4];
    if magic == b"ttcf" {
        if header.len() < 16 || total_len < 16 {
            return false;
        }
        let num_fonts = u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
        if num_fonts == 0 || total_len < 12 + num_fonts * 4 {
            return false;
        }
        let first_offset =
            u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as usize;
        return first_offset + 12 <= total_len;
    }

    if !is_valid_sfnt_magic(magic) {
        return false;
    }
    let num_tables = u16::from_be_bytes([header[4], header[5]]) as usize;
    num_tables > 0 && total_len >= 12 + num_tables * 16
}

/// Validates TrueType / OpenType / TTC header magic and table bounds (Section 4.3.2).
pub fn is_valid_font_bytes(bytes: &[u8]) -> bool {
    check_font_header_bounds(bytes, bytes.len())
}

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
        Some(check_font_header_bounds(&header, file_len))
    })()
    .unwrap_or(false);

    if let Ok(mut guard) = cache_mutex.lock() {
        guard.insert(path.to_path_buf(), valid);
    }
    valid
}

/// Locates the `(offset, length)` of the `name` table within an SFNT header slice.
fn find_name_table_in_sfnt(sfnt_buf: &[u8]) -> Option<(usize, usize)> {
    if sfnt_buf.len() < 12 || !is_valid_sfnt_magic(&sfnt_buf[0..4]) {
        return None;
    }
    let num_tables = u16::from_be_bytes([sfnt_buf[4], sfnt_buf[5]]) as usize;
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
            return Some((offset, length));
        }
    }
    None
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

    let (table_start, name_table_len) = find_name_table_in_sfnt(&sfnt_buf)?;
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
        let base = stem.split('-').next().unwrap_or(stem).replace('_', " ");
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

    let (table_start, name_table_len) = find_name_table_in_sfnt(&bytes[sfnt_offset..])?;
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
            } else if name_id == 1 && (standard_family.is_none() || is_english) {
                standard_family = Some(clean);
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
