// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

#![allow(deprecated)]

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use gtk4::pango;
use gtk4::prelude::*;

use super::PreferencesWindow;
use super::widgets::{build_labeled_row, disable_combo_scroll};
use crate::config::theme::{ThemeSettings, cached_system_fonts};
use crate::i18n::tr;

const MAX_COMBO_FONTS: usize = 200;

impl PreferencesWindow {
    pub(super) fn build_font_tab(self: &Rc<Self>) -> gtk4::Widget {
        self.ensure_fonts_loaded();

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        let s = self.workspace.settings();

        let filter_row = build_labeled_row(&tr("Font Filter Keyword"), 22);
        let filter_entry = gtk4::Entry::new();
        filter_entry.set_hexpand(true);
        let rescan_btn = gtk4::Button::with_label(&tr("Rescan"));
        filter_row.append(&filter_entry);
        filter_row.append(&rescan_btn);
        vbox.append(&filter_row);

        let catalog_row = build_labeled_row("", 22);
        let catalog_btn =
            gtk4::Button::with_label(&format!("🗚 {}", tr("Open Font Sample Catalog...")));
        catalog_btn.set_hexpand(true);
        catalog_row.append(&catalog_btn);
        vbox.append(&catalog_row);

        let combo_row = build_labeled_row(&tr("System Font"), 22);
        let font_combo = gtk4::ComboBoxText::new();
        font_combo.set_hexpand(true);
        disable_combo_scroll(&font_combo);
        combo_row.append(&font_combo);
        vbox.append(&combo_row);

        let path_row = build_labeled_row(&tr("Font File Path"), 22);
        let path_entry = gtk4::Entry::new();
        path_entry.set_text(&s.font_path);
        path_entry.set_hexpand(true);
        path_row.append(&path_entry);
        vbox.append(&path_row);

        {
            let p_weak = Rc::downgrade(self);
            let pe = path_entry.clone();
            catalog_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    p.open_font_sample_catalog(pe.clone());
                }
            });
        }

        let combo_populating = Rc::new(Cell::new(false));

        let populate_combo = {
            let fonts_rc = Rc::clone(&self.discovered_fonts);
            let combo_clone = font_combo.clone();
            let pop_flag = Rc::clone(&combo_populating);
            move |kw: &str, current_path: &str| {
                pop_flag.set(true);
                combo_clone.remove_all();
                let kw_lower = kw.trim().to_ascii_lowercase();
                let mut count = 0usize;
                let mut current_added = current_path.trim().is_empty();

                for path in fonts_rc.borrow().iter() {
                    let p_str = path.to_string_lossy();
                    if !kw_lower.is_empty() && !p_str.to_ascii_lowercase().contains(&kw_lower) {
                        continue;
                    }
                    let label = path.file_name().and_then(|n| n.to_str()).unwrap_or(&p_str);
                    combo_clone.append(Some(&p_str), label);
                    if p_str == current_path {
                        current_added = true;
                    }
                    count += 1;
                    if count >= MAX_COMBO_FONTS {
                        break;
                    }
                }

                if !current_added && Path::new(current_path).exists() {
                    let label = Path::new(current_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(current_path);
                    combo_clone.append(Some(current_path), label);
                }

                if !current_path.trim().is_empty() {
                    let _ = combo_clone.set_active_id(Some(current_path));
                }
                pop_flag.set(false);
            }
        };
        populate_combo("", &s.font_path);

        let preview_frame = gtk4::Frame::new(Some(&tr("Font Preview")));
        let preview_label = gtk4::Label::new(Some(
            "ASCII: ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 !@#$%^&*()\n日本語: 日本語テキストの表示テスト（コピー・ペースト・ペイン分割）",
        ));
        preview_label.set_margin_top(10);
        preview_label.set_margin_bottom(10);
        preview_label.set_margin_start(10);
        preview_label.set_margin_end(10);
        preview_label.set_halign(gtk4::Align::Start);
        preview_label.set_ellipsize(pango::EllipsizeMode::End);
        preview_frame.set_child(Some(&preview_label));

        let update_preview = {
            let lbl_clone = preview_label.clone();
            move |st: &ThemeSettings| {
                let fam = st.effective_font_family();
                let sz = st.effective_terminal_font_size();
                let attrs = pango::AttrList::new();
                let mut desc = pango::FontDescription::new();
                desc.set_family(&format!("{fam}, Monospace"));
                desc.set_size(((sz as f64).max(6.0) * pango::SCALE as f64) as i32);
                attrs.insert(pango::AttrFontDesc::new(&desc));
                lbl_clone.set_attributes(Some(&attrs));
            }
        };
        update_preview(&s);

        {
            let pop = populate_combo.clone();
            let pe = path_entry.clone();
            filter_entry.connect_changed(move |e| {
                pop(&e.text(), &pe.text());
            });
        }
        {
            let fonts_rc = Rc::clone(&self.discovered_fonts);
            let pop = populate_combo.clone();
            let fe = filter_entry.clone();
            let pe = path_entry.clone();
            rescan_btn.connect_clicked(move |_| {
                *fonts_rc.borrow_mut() = cached_system_fonts(true);
                pop(&fe.text(), &pe.text());
            });
        }
        {
            let pe = path_entry.clone();
            let pop_flag = Rc::clone(&combo_populating);
            font_combo.connect_changed(move |c| {
                if pop_flag.get() {
                    return;
                }
                if let Some(id) = c.active_id()
                    && pe.text() != id
                {
                    pe.set_text(&id);
                }
            });
        }
        {
            let p_weak = Rc::downgrade(self);
            let upd_prev = update_preview.clone();
            path_entry.connect_changed(move |e| {
                let txt = e.text().to_string();
                let trimmed = txt.trim();
                let ready =
                    trimmed.is_empty() || !trimmed.contains('/') || Path::new(trimmed).is_file();
                if ready && let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    st.font_path = txt;
                    upd_prev(&st);
                    p.notify_changed(st);
                }
            });
        }

        self.add_slider_row(
            &vbox,
            &tr("UI / Menu Font Size (pt)"),
            s.effective_ui_font_size().clamp(9.0, 28.0) as f64,
            9.0,
            28.0,
            0.5,
            1,
            |st, v| st.ui_font_size = v as f32,
        );
        {
            let upd_prev = update_preview.clone();
            self.add_slider_row(
                &vbox,
                &tr("Terminal Font Size (pt)"),
                s.effective_terminal_font_size().clamp(8.0, 36.0) as f64,
                8.0,
                36.0,
                0.5,
                1,
                move |st, v| {
                    st.terminal_font_size = v as f32;
                    upd_prev(st);
                },
            );
        }

        vbox.append(&preview_frame);
        vbox.upcast()
    }
}
