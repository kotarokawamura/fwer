// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

#![allow(deprecated)]

use std::rc::Rc;

use gtk4::prelude::*;

use super::PreferencesWindow;
use super::widgets::{build_labeled_row, disable_combo_scroll};
use crate::i18n::{
    active_entry_count, active_locale, discover_po_files, load_catalog_for_language, tr,
};

impl PreferencesWindow {
    pub(super) fn build_language_tab(self: &Rc<Self>) -> gtk4::Widget {
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        let s = self.workspace.settings();

        let lang_row = build_labeled_row(&tr("Display Language"), 24);
        let lang_combo = gtk4::ComboBoxText::new();
        lang_combo.append(Some("auto"), &tr("Auto (System Locale)"));
        lang_combo.append(Some("en"), &tr("English (en.po)"));
        lang_combo.append(Some("ja"), &tr("日本語 (ja.po)"));
        for (code, path) in discover_po_files() {
            if code != "en" && code != "ja" {
                lang_combo.append(Some(&code), &format!("{} ({})", code, path.display()));
            }
        }
        let _ = lang_combo.set_active_id(Some(&s.language));
        lang_combo.set_hexpand(true);
        disable_combo_scroll(&lang_combo);
        lang_row.append(&lang_combo);
        vbox.append(&lang_row);

        let custom_row = build_labeled_row(&tr("Custom .po File Path"), 24);
        let custom_entry = gtk4::Entry::new();
        custom_entry.set_text(&s.custom_po_path);
        custom_entry.set_hexpand(true);
        let reload_btn = gtk4::Button::with_label(&tr("Reload .po Files"));
        custom_row.append(&custom_entry);
        custom_row.append(&reload_btn);
        vbox.append(&custom_row);

        let status_lbl = gtk4::Label::new(Some(&format!(
            "{}: {}  |  {}: {}",
            tr("Active Locale"),
            active_locale(),
            tr("Loaded Translation Entries"),
            active_entry_count()
        )));
        status_lbl.set_halign(gtk4::Align::Start);
        vbox.append(&status_lbl);

        {
            let p_weak = Rc::downgrade(self);
            lang_combo.connect_changed(move |c| {
                if let Some(id) = c.active_id()
                    && let Some(p) = p_weak.upgrade()
                {
                    let mut st = p.workspace.settings();
                    st.language = id.to_string();
                    load_catalog_for_language(&st.language, &st.custom_po_path);
                    p.notify_changed(st);
                    p.window.set_title(Some(&tr("Termin Preferences")));
                    p.rebuild_tabs();
                }
            });
        }
        {
            let p_weak = Rc::downgrade(self);
            let ce = custom_entry.clone();
            reload_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    st.custom_po_path = ce.text().to_string();
                    load_catalog_for_language(&st.language, &st.custom_po_path);
                    p.notify_changed(st);
                    p.window.set_title(Some(&tr("Termin Preferences")));
                    p.rebuild_tabs();
                }
            });
        }

        vbox.upcast()
    }
}
