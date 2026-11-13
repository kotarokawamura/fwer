// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::{PreferencesWindow, RecordingTarget, ShortcutRowWidgets};
use crate::config::theme::{ShortcutSettings, split_shortcut_bindings};
use crate::i18n::tr;

impl PreferencesWindow {
    pub(super) fn refresh_shortcut_row_ui(self: &Rc<Self>, slot_key: &str) {
        let val = self.workspace.settings().shortcuts.get_slot(slot_key);
        let row_opt = self.shortcut_rows.borrow().get(slot_key).cloned();
        if let Some(row_w) = row_opt {
            if row_w.entry.text() != val {
                row_w.updating.set(true);
                row_w.entry.set_text(&val);
                row_w.updating.set(false);
            }
            self.rebuild_row_chips(&row_w.chips_box, slot_key, &val);
        }
    }

    pub(super) fn refresh_all_shortcut_rows_ui(self: &Rc<Self>) {
        let keys: Vec<String> = self.shortcut_rows.borrow().keys().cloned().collect();
        for k in keys {
            self.refresh_shortcut_row_ui(&k);
        }
    }

    pub(super) fn rebuild_row_chips(
        self: &Rc<Self>,
        chips_box: &gtk4::Box,
        slot_key: &str,
        current_val: &str,
    ) {
        while let Some(child) = chips_box.first_child() {
            chips_box.remove(&child);
        }
        let bindings = split_shortcut_bindings(current_val);
        if bindings.len() > 1 {
            for (idx, b_str) in bindings.iter().enumerate() {
                let chip_btn = gtk4::Button::with_label(&format!("{} ✕", b_str));
                let p_weak = Rc::downgrade(self);
                let key_owned = slot_key.to_string();
                let all_bindings = bindings.clone();
                chip_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let remaining: Vec<String> = all_bindings
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != idx)
                            .map(|(_, v)| v.clone())
                            .collect();
                        let joined = remaining.join(" / ");
                        let mut st = p.workspace.settings();
                        st.shortcuts.update_slot(&key_owned, &joined, false);
                        p.notify_changed(st);
                        p.refresh_shortcut_row_ui(&key_owned);
                    }
                });
                chips_box.append(&chip_btn);
            }
        }
    }

    fn start_shortcut_recording(&self, slot_key: &str, append: bool) {
        *self.recording.borrow_mut() = Some(RecordingTarget {
            slot_key: slot_key.to_string(),
            append,
        });
        self.recording_status_label
            .set_text(&tr("Press shortcut keys... (Esc to cancel)"));
        self.recording_status_label.set_visible(true);
    }

    pub(super) fn build_shortcuts_tab(self: &Rc<Self>) -> gtk4::Widget {
        self.shortcut_rows.borrow_mut().clear();

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        let s = self.workspace.settings();

        for (slot_key, label, current_val, default_val) in s.shortcuts.slots() {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

            let lbl = gtk4::Label::new(Some(&tr(label)));
            lbl.set_width_chars(24);
            lbl.set_halign(gtk4::Align::Start);
            row.append(&lbl);

            let entry = gtk4::Entry::new();
            entry.set_placeholder_text(Some(&tr("Disabled")));
            entry.set_text(current_val);
            entry.set_hexpand(true);
            row.append(&entry);

            let chips_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            self.rebuild_row_chips(&chips_box, slot_key, current_val);
            row.append(&chips_box);

            let updating = Rc::new(Cell::new(false));
            self.shortcut_rows.borrow_mut().insert(
                slot_key.to_string(),
                ShortcutRowWidgets {
                    entry: entry.clone(),
                    chips_box: chips_box.clone(),
                    updating: Rc::clone(&updating),
                },
            );

            let record_btn = gtk4::Button::with_label(&tr("Record"));
            {
                let p_weak = Rc::downgrade(self);
                let key_owned = slot_key.to_string();
                record_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        p.start_shortcut_recording(&key_owned, false);
                    }
                });
            }
            row.append(&record_btn);

            let add_btn = gtk4::Button::with_label(&tr("+ Add"));
            {
                let p_weak = Rc::downgrade(self);
                let key_owned = slot_key.to_string();
                add_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        p.start_shortcut_recording(&key_owned, true);
                    }
                });
            }
            row.append(&add_btn);

            let disable_btn = gtk4::Button::with_label(&tr("Disable"));
            {
                let p_weak = Rc::downgrade(self);
                let key_owned = slot_key.to_string();
                disable_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        *p.recording.borrow_mut() = None;
                        p.recording_status_label.set_visible(false);
                        let mut st = p.workspace.settings();
                        st.shortcuts.update_slot(&key_owned, "", false);
                        p.notify_changed(st);
                        p.refresh_shortcut_row_ui(&key_owned);
                    }
                });
            }
            row.append(&disable_btn);

            let def_btn = gtk4::Button::with_label(&tr("Default"));
            {
                let p_weak = Rc::downgrade(self);
                let key_owned = slot_key.to_string();
                let def_owned = default_val.to_string();
                def_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = p.workspace.settings();
                        st.shortcuts.update_slot(&key_owned, &def_owned, false);
                        p.notify_changed(st);
                        p.refresh_shortcut_row_ui(&key_owned);
                    }
                });
            }
            row.append(&def_btn);

            {
                let p_weak = Rc::downgrade(self);
                let key_owned = slot_key.to_string();
                let upd = Rc::clone(&updating);
                let cb_clone = chips_box.clone();
                entry.connect_changed(move |e| {
                    if upd.get() {
                        return;
                    }
                    if let Some(p) = p_weak.upgrade() {
                        let txt = e.text().to_string();
                        let mut st = p.workspace.settings();
                        st.shortcuts.update_slot(&key_owned, &txt, false);
                        p.notify_changed(st);
                        p.rebuild_row_chips(&cb_clone, &key_owned, &txt);
                    }
                });
            }

            vbox.append(&row);
        }

        vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let reset_all_btn = gtk4::Button::with_label(&tr("Reset Shortcuts to Defaults"));
        {
            let p_weak = Rc::downgrade(self);
            reset_all_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    st.shortcuts = ShortcutSettings::default();
                    p.notify_changed(st);
                    p.refresh_all_shortcut_rows_ui();
                }
            });
        }
        vbox.append(&reset_all_btn);

        vbox.upcast()
    }
}
