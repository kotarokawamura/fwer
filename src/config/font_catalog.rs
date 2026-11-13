// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::{
    glib, pango, prelude::*, Box as GtkBox, Button, Entry, Label, Orientation, ScrolledWindow,
    Separator, ToggleButton, Window,
};

use super::widgets::build_labeled_row;
use super::PreferencesWindow;
use crate::config::theme::{
    cached_system_fonts, extract_font_family_from_path, is_font_cjk, is_font_monospace,
    is_valid_font_file,
};
use crate::i18n::tr;

impl PreferencesWindow {
    pub(super) fn open_font_sample_catalog(self: &Rc<Self>, path_entry_to_sync: Entry) {
        self.ensure_fonts_loaded();

        if let Some(existing) = self.catalog_window.borrow().as_ref() {
            existing.present();
            return;
        }

        let cat_win = Window::builder()
            .title(tr("System Font Samples & Selector"))
            .transient_for(&self.window)
            .default_width(760)
            .default_height(540)
            .width_request(560)
            .height_request(380)
            .build();

        cat_win.add_css_class("fwer-prefs-window");
        let root = GtkBox::new(Orientation::Vertical, 8);
        root.set_margin_top(10);
        root.set_margin_bottom(10);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let desc_lbl = Label::new(Some(&tr(
            "Browse installed system fonts with live text samples and click 'Use This Font' to apply.",
        )));
        desc_lbl.set_halign(gtk4::Align::Start);
        desc_lbl.add_css_class("dim-label");
        root.append(&desc_lbl);

        let cat_row = build_labeled_row(&tr("Category:"), 18);
        let btn_all = ToggleButton::with_label(&tr("All Fonts"));
        let btn_mono = ToggleButton::with_label(&tr("Monospace"));
        let btn_cjk = ToggleButton::with_label(&tr("Japanese / CJK"));
        btn_mono.set_group(Some(&btn_all));
        btn_cjk.set_group(Some(&btn_all));
        btn_all.set_active(true);

        cat_row.append(&btn_all);
        cat_row.append(&btn_mono);
        cat_row.append(&btn_cjk);
        root.append(&cat_row);

        let filter_row = build_labeled_row(&tr("Font Filter Keyword"), 18);
        let search_entry = Entry::new();
        search_entry.set_placeholder_text(Some("noto, mono, vl, gothic..."));
        search_entry.set_hexpand(true);
        let rescan_btn = Button::with_label(&tr("Rescan"));
        filter_row.append(&search_entry);
        filter_row.append(&rescan_btn);
        root.append(&filter_row);

        let sample_row = build_labeled_row(&tr("Sample Text"), 18);
        let sample_ascii_entry = Entry::new();
        sample_ascii_entry.set_text(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789 !@#$%^&*()",
        );
        sample_ascii_entry.set_hexpand(true);
        let sample_cjk_entry = Entry::new();
        sample_cjk_entry.set_text(
            "日本語サンプル: 吾輩は猫である。名前はまだ無い。 (漢字・ひらがな・カタカナ)",
        );
        sample_cjk_entry.set_hexpand(true);
        sample_row.append(&sample_ascii_entry);
        sample_row.append(&sample_cjk_entry);
        root.append(&sample_row);

        let sep_top = Separator::new(Orientation::Horizontal);
        root.append(&sep_top);

        let matching_lbl = Label::new(None);
        matching_lbl.set_halign(gtk4::Align::Start);
        root.append(&matching_lbl);

        let list_box = GtkBox::new(Orientation::Vertical, 6);
        let scrolled = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&list_box)
            .build();
        root.append(&scrolled);

        let sep_bottom = Separator::new(Orientation::Horizontal);
        root.append(&sep_bottom);

        let footer = GtkBox::new(Orientation::Horizontal, 8);
        let current_font_lbl = Label::new(Some(&format!(
            "{}: {}",
            tr("Current Font"),
            self.workspace.settings().font_path
        )));
        current_font_lbl.set_halign(gtk4::Align::Start);
        current_font_lbl.set_hexpand(true);
        current_font_lbl.set_ellipsize(pango::EllipsizeMode::Middle);
        let close_btn = Button::with_label(&tr("Close"));
        footer.append(&current_font_lbl);
        footer.append(&close_btn);
        root.append(&footer);

        let live_ascii_labels: Rc<RefCell<Vec<Label>>> = Rc::new(RefCell::new(Vec::new()));
        let live_cjk_labels: Rc<RefCell<Vec<Label>>> = Rc::new(RefCell::new(Vec::new()));

        type RebuildFn = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
        let rebuild_holder: RebuildFn = Rc::new(RefCell::new(None));
        let rebuild_holder_clone = Rc::clone(&rebuild_holder);

        let rebuild_catalog_rows: Rc<dyn Fn()> = {
            let fonts_rc = Rc::clone(&self.discovered_fonts);
            let list_clone = list_box.clone();
            let btn_mono_c = btn_mono.clone();
            let btn_cjk_c = btn_cjk.clone();
            let search_clone = search_entry.clone();
            let ascii_entry_c = sample_ascii_entry.clone();
            let cjk_entry_c = sample_cjk_entry.clone();
            let matching_lbl_c = matching_lbl.clone();
            let current_font_lbl_c = current_font_lbl.clone();
            let ascii_labels_rc = Rc::clone(&live_ascii_labels);
            let cjk_labels_rc = Rc::clone(&live_cjk_labels);
            let p_weak = Rc::downgrade(self);
            let pe_sync = path_entry_to_sync;

            Rc::new(move || {
                while let Some(c) = list_clone.first_child() {
                    list_clone.remove(&c);
                }
                ascii_labels_rc.borrow_mut().clear();
                cjk_labels_rc.borrow_mut().clear();

                let filter_mono = btn_mono_c.is_active();
                let filter_cjk = btn_cjk_c.is_active();
                let kw = search_clone.text().trim().to_ascii_lowercase();
                let ascii_txt = ascii_entry_c.text().to_string();
                let cjk_txt = cjk_entry_c.text().to_string();

                let current_selected = p_weak
                    .upgrade()
                    .map(|p| p.workspace.settings().font_path)
                    .unwrap_or_default();
                current_font_lbl_c.set_text(&format!(
                    "{}: {}",
                    tr("Current Font"),
                    current_selected
                ));

                let all_fonts = fonts_rc.borrow();
                let total_count = all_fonts.len();
                let mut matching_paths = Vec::new();

                for path in all_fonts.iter() {
                    if filter_mono && !is_font_monospace(path) {
                        continue;
                    }
                    if filter_cjk && !is_font_cjk(path) {
                        continue;
                    }
                    let path_str = path.to_string_lossy();
                    if !kw.is_empty() && !path_str.to_ascii_lowercase().contains(&kw) {
                        continue;
                    }
                    matching_paths.push(path.clone());
                }

                matching_lbl_c.set_text(&format!(
                    "{}: {} / {}",
                    tr("Matching Fonts"),
                    matching_paths.len(),
                    total_count
                ));

                let mut shown = 0usize;
                for path in matching_paths.iter() {
                    if shown >= 96 {
                        break;
                    }
                    if !is_valid_font_file(path) {
                        continue;
                    }
                    let path_str = path.to_string_lossy().to_string();
                    let font_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&path_str);
                    let is_selected = current_selected == path_str;

                    let card = GtkBox::new(Orientation::Vertical, 4);
                    card.add_css_class(if is_selected {
                        "fwer-font-card-selected"
                    } else {
                        "fwer-font-card"
                    });

                    let header_row = GtkBox::new(Orientation::Horizontal, 8);
                    let name_lbl = Label::new(None);
                    name_lbl.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(font_name)));
                    name_lbl.set_halign(gtk4::Align::Start);
                    header_row.append(&name_lbl);

                    if is_font_monospace(path) {
                        let mono_badge = Label::new(None);
                        mono_badge.set_markup("<span foreground=\"#60a5fa\"><b>[Mono]</b></span>");
                        header_row.append(&mono_badge);
                    }
                    if is_font_cjk(path) {
                        let cjk_badge = Label::new(None);
                        cjk_badge.set_markup("<span foreground=\"#facc15\"><b>[CJK]</b></span>");
                        header_row.append(&cjk_badge);
                    }

                    let path_dim = Label::new(Some(&path_str));
                    path_dim.add_css_class("dim-label");
                    path_dim.set_halign(gtk4::Align::Start);
                    path_dim.set_hexpand(true);
                    path_dim.set_ellipsize(pango::EllipsizeMode::Middle);
                    header_row.append(&path_dim);

                    if is_selected {
                        let sel_lbl = Label::new(None);
                        sel_lbl.set_markup(&format!(
                            "<span foreground=\"#4ade80\"><b>✔ {}</b></span>",
                            glib::markup_escape_text(&tr("Selected"))
                        ));
                        header_row.append(&sel_lbl);
                    } else {
                        let use_btn = Button::with_label(&tr("Use This Font"));
                        let p_w = p_weak.clone();
                        let pe = pe_sync.clone();
                        let chosen = path_str.clone();
                        let reb_ref = Rc::clone(&rebuild_holder_clone);
                        use_btn.connect_clicked(move |_| {
                            pe.set_text(&chosen);
                            if let Some(p) = p_w.upgrade() {
                                let mut st = p.workspace.settings();
                                st.font_path = chosen.clone();
                                p.notify_changed(st);
                            }
                            if let Some(ref reb) = *reb_ref.borrow() {
                                reb();
                            }
                        });
                        header_row.append(&use_btn);
                    }
                    card.append(&header_row);

                    let fam = extract_font_family_from_path(&path_str)
                        .unwrap_or_else(|| "Monospace".to_string());
                    let attrs = pango::AttrList::new();
                    let mut desc = pango::FontDescription::new();
                    desc.set_family(&format!("{fam}, Monospace"));
                    desc.set_size((11.0 * pango::SCALE as f64) as i32);
                    attrs.insert(pango::AttrFontDesc::new(&desc));

                    let ascii_lbl = Label::new(Some(&ascii_txt));
                    ascii_lbl.set_halign(gtk4::Align::Start);
                    ascii_lbl.set_ellipsize(pango::EllipsizeMode::End);
                    ascii_lbl.set_attributes(Some(&attrs));

                    let cjk_lbl = Label::new(Some(&cjk_txt));
                    cjk_lbl.set_halign(gtk4::Align::Start);
                    cjk_lbl.set_ellipsize(pango::EllipsizeMode::End);
                    cjk_lbl.set_attributes(Some(&attrs));

                    card.append(&ascii_lbl);
                    card.append(&cjk_lbl);

                    ascii_labels_rc.borrow_mut().push(ascii_lbl);
                    cjk_labels_rc.borrow_mut().push(cjk_lbl);

                    list_clone.append(&card);
                    shown += 1;
                }
            })
        };

        *rebuild_holder.borrow_mut() = Some(Rc::clone(&rebuild_catalog_rows));
        rebuild_catalog_rows();

        for btn in [&btn_all, &btn_mono, &btn_cjk] {
            let r = Rc::clone(&rebuild_catalog_rows);
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    r();
                }
            });
        }
        {
            let r = Rc::clone(&rebuild_catalog_rows);
            search_entry.connect_changed(move |_| r());
        }
        {
            let fonts_rc = Rc::clone(&self.discovered_fonts);
            let r = Rc::clone(&rebuild_catalog_rows);
            rescan_btn.connect_clicked(move |_| {
                *fonts_rc.borrow_mut() = cached_system_fonts(true);
                r();
            });
        }
        {
            let labels_rc = Rc::clone(&live_ascii_labels);
            sample_ascii_entry.connect_changed(move |e| {
                let txt = e.text();
                for lbl in labels_rc.borrow().iter() {
                    lbl.set_text(&txt);
                }
            });
        }
        {
            let labels_rc = Rc::clone(&live_cjk_labels);
            sample_cjk_entry.connect_changed(move |e| {
                let txt = e.text();
                for lbl in labels_rc.borrow().iter() {
                    lbl.set_text(&txt);
                }
            });
        }
        {
            let w_clone = cat_win.clone();
            close_btn.connect_clicked(move |_| {
                w_clone.close();
            });
        }

        cat_win.set_child(Some(&root));
        {
            let p_weak = Rc::downgrade(self);
            cat_win.connect_close_request(move |w| {
                if let Some(p) = p_weak.upgrade() {
                    *p.catalog_window.borrow_mut() = None;
                }
                w.destroy();
                glib::Propagation::Stop
            });
        }

        *self.catalog_window.borrow_mut() = Some(cat_win.clone());
        cat_win.present();
    }
}
