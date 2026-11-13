// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

#![allow(deprecated)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;

use super::widgets::{build_labeled_row, build_optional_scale_row, disable_combo_scroll};
use super::{PreferencesWindow, PresetEditorTarget};
use crate::config::layout_preset::{
    LayoutNode, PanePresetConfig, PresetStore, WorkspacePreset,
};
use crate::i18n::tr;
use crate::pane::PaneKind;

/// 指定インデックスのプリセットを読み込み・変更・保存する共通ヘルパー
fn mutate_preset(selected_idx: usize, f: impl FnOnce(&mut WorkspacePreset)) {
    let mut st = PresetStore::load();
    if let Some(pr) = st.presets.get_mut(selected_idx) {
        f(pr);
        let _ = st.save();
    }
}

/// 指定インデックスのプリセット内ペイン設定を読み込み・変更・保存する共通ヘルパー
fn mutate_preset_pane(selected_idx: usize, pane_idx: usize, f: impl FnMut(&mut PanePresetConfig)) {
    mutate_preset(selected_idx, |pr| {
        pr.root.update_pane_at(pane_idx, f);
    });
}

impl PreferencesWindow {
    /// Column 2 下部と Column 3 上部で重複していた「水平分割・垂直分割・タブ追加・ペイン削除」ボタン群を共通生成
    fn build_pane_structure_actions(
        self: &Rc<Self>,
        selected_idx: usize,
        pane_idx: usize,
        total_panes: usize,
        compact: bool,
    ) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, if compact { 4 } else { 6 });

        let (sh_lbl, sv_lbl, tab_lbl, rm_lbl) = if compact {
            (
                "↔ Split H".to_string(),
                "↕ Split V".to_string(),
                "➕ Tab".to_string(),
                "✖".to_string(),
            )
        } else {
            (
                tr("Split Horizontally"),
                tr("Split Vertically"),
                tr("Add Tab"),
                tr("Remove Pane"),
            )
        };

        let sh_btn = gtk4::Button::with_label(&sh_lbl);
        let sv_btn = gtk4::Button::with_label(&sv_lbl);
        let tab_btn = gtk4::Button::with_label(&tab_lbl);
        let rm_btn = gtk4::Button::with_label(&rm_lbl);
        rm_btn.set_sensitive(total_panes > 1);

        if compact {
            sh_btn.set_tooltip_text(Some(&tr("Split Horizontally")));
            sh_btn.set_hexpand(true);
            sv_btn.set_tooltip_text(Some(&tr("Split Vertically")));
            sv_btn.set_hexpand(true);
            tab_btn.set_tooltip_text(Some(&tr("Add Tab")));
            rm_btn.set_tooltip_text(Some(&tr("Remove Pane")));
        }

        {
            let p_weak = Rc::downgrade(self);
            sh_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    mutate_preset(selected_idx, |pr| {
                        pr.root.split_pane_at(pane_idx, true);
                    });
                    p.selected_preset_target
                        .set(PresetEditorTarget::Pane(pane_idx + 1));
                    p.refresh_presets_ui();
                }
            });
        }
        {
            let p_weak = Rc::downgrade(self);
            sv_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    mutate_preset(selected_idx, |pr| {
                        pr.root.split_pane_at(pane_idx, false);
                    });
                    p.selected_preset_target
                        .set(PresetEditorTarget::Pane(pane_idx + 1));
                    p.refresh_presets_ui();
                }
            });
        }
        {
            let p_weak = Rc::downgrade(self);
            tab_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    mutate_preset(selected_idx, |pr| {
                        pr.root.add_tab_at(pane_idx);
                    });
                    p.selected_preset_target
                        .set(PresetEditorTarget::Pane(pane_idx + 1));
                    p.refresh_presets_ui();
                }
            });
        }
        {
            let p_weak = Rc::downgrade(self);
            rm_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    mutate_preset(selected_idx, |pr| {
                        pr.root.remove_pane_at(pane_idx);
                    });
                    p.selected_preset_target
                        .set(PresetEditorTarget::Pane(pane_idx.saturating_sub(1)));
                    p.refresh_presets_ui();
                }
            });
        }

        row.append(&sh_btn);
        row.append(&sv_btn);
        row.append(&tab_btn);
        row.append(&rm_btn);
        row
    }

    pub(super) fn refresh_presets_ui(self: &Rc<Self>) {
        while let Some(child) = self.presets_box.first_child() {
            self.presets_box.remove(&child);
        }

        let store = PresetStore::load();
        if !store.presets.is_empty() {
            let cur_idx = self.selected_preset_index.get().unwrap_or(0);
            if cur_idx >= store.presets.len() {
                self.selected_preset_index
                    .set(Some(store.presets.len() - 1));
            } else if self.selected_preset_index.get().is_none() {
                self.selected_preset_index.set(Some(0));
            }
        } else {
            self.selected_preset_index.set(None);
        }

        let selected_idx_opt = self.selected_preset_index.get();
        let selected_preset_opt = selected_idx_opt.and_then(|i| store.presets.get(i).cloned());

        if let Some(ref pr) = selected_preset_opt {
            let pane_count = pr.root.pane_count().max(1);
            if let PresetEditorTarget::Pane(p_idx) = self.selected_preset_target.get()
                && p_idx >= pane_count
            {
                self.selected_preset_target
                    .set(PresetEditorTarget::Pane(pane_count - 1));
            }
        } else {
            self.selected_preset_target.set(PresetEditorTarget::Window);
        }

        let target = self.selected_preset_target.get();

        // Top header + breadcrumb
        let header_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        let top_title = gtk4::Label::new(None);
        top_title.set_markup(&format!("<b>{}</b>", tr("Workspace Presets")));
        top_title.set_halign(gtk4::Align::Start);
        header_row.append(&top_title);

        let breadcrumb_lbl = gtk4::Label::new(None);
        if let Some(ref pr) = selected_preset_opt {
            let target_str = match target {
                PresetEditorTarget::Window => format!("<b>🪟 {}</b>", tr("Window")),
                PresetEditorTarget::Pane(p_idx) => {
                    let cfgs = pr.root.pane_configs();
                    let p_cfg = cfgs.get(p_idx);
                    let p_title = p_cfg
                        .map(|c| c.title.clone())
                        .unwrap_or_else(|| "Shell".to_string());
                    let p_icon = p_cfg.map(|c| c.pane_kind.icon()).unwrap_or("🖥");
                    format!(
                        "🪟 {}  →  <b>{} {} #{} ({})</b>",
                        tr("Window"),
                        p_icon,
                        tr("Pane"),
                        p_idx + 1,
                        glib::markup_escape_text(&p_title)
                    )
                }
            };
            breadcrumb_lbl.set_markup(&format!(
                "<small>🗂 <b>{}</b>  →  {}</small>",
                glib::markup_escape_text(&pr.name),
                target_str
            ));
        } else {
            breadcrumb_lbl.set_markup(&format!("<small>{}</small>", tr("No Saved Presets")));
        }
        breadcrumb_lbl.set_halign(gtk4::Align::End);
        breadcrumb_lbl.set_hexpand(true);
        header_row.append(&breadcrumb_lbl);
        self.presets_box.append(&header_row);

        let columns_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        columns_row.set_hexpand(true);
        columns_row.set_vexpand(true);

        // ====================================================================
        // COLUMN 1 (LEFT): Saved Presets List & Create/Save/Delete Controls
        // ====================================================================
        let col1 = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        col1.add_css_class("fwer-preset-col");
        col1.set_width_request(215);
        col1.set_vexpand(true);

        let col1_title = gtk4::Label::new(None);
        col1_title.set_markup(&format!("<b>1. {}</b>", tr("Saved Presets")));
        col1_title.set_halign(gtk4::Align::Start);
        col1.append(&col1_title);

        let col1_scroll = gtk4::ScrolledWindow::new();
        col1_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        col1_scroll.set_vexpand(true);
        col1_scroll.set_min_content_height(220);

        let preset_list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        if store.presets.is_empty() {
            let empty_lbl = gtk4::Label::new(Some(&tr("No Saved Presets")));
            empty_lbl.set_halign(gtk4::Align::Start);
            empty_lbl.set_margin_top(8);
            preset_list_box.append(&empty_lbl);
        } else {
            let sel_idx = selected_idx_opt.unwrap_or(0);
            for (idx, preset) in store.presets.iter().enumerate() {
                let is_sel = idx == sel_idx;
                let item_btn = gtk4::Button::new();
                item_btn.add_css_class("flat");
                item_btn.add_css_class(if is_sel {
                    "fwer-preset-item-selected"
                } else {
                    "fwer-preset-item"
                });

                let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                let startup_badge = if preset.run_on_startup { " ★" } else { "" };
                let lbl = gtk4::Label::new(None);
                lbl.set_markup(&format!(
                    "<b>{}</b>{} <small>({})</small>",
                    glib::markup_escape_text(&preset.name),
                    startup_badge,
                    preset.root.pane_count()
                ));
                lbl.set_halign(gtk4::Align::Start);
                lbl.set_hexpand(true);
                lbl.set_ellipsize(pango::EllipsizeMode::End);
                row_box.append(&lbl);

                if is_sel {
                    let arrow = gtk4::Label::new(Some("→"));
                    arrow.set_halign(gtk4::Align::End);
                    row_box.append(&arrow);
                }

                item_btn.set_child(Some(&row_box));
                {
                    let p_weak = Rc::downgrade(self);
                    item_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            p.selected_preset_index.set(Some(idx));
                            p.selected_preset_target.set(PresetEditorTarget::Window);
                            p.refresh_presets_ui();
                        }
                    });
                }
                preset_list_box.append(&item_btn);
            }
        }
        col1_scroll.set_child(Some(&preset_list_box));
        col1.append(&col1_scroll);

        col1.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let name_entry = gtk4::Entry::new();
        name_entry.set_placeholder_text(Some(&tr("Preset Name (e.g. Dev Set)")));
        col1.append(&name_entry);

        let add_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        let new_preset_btn = gtk4::Button::with_label(&tr("+ New Preset"));
        new_preset_btn.set_hexpand(true);
        let save_cur_btn = gtk4::Button::with_label(&tr("Save Current Workspace"));
        save_cur_btn.set_hexpand(true);
        add_row.append(&new_preset_btn);
        add_row.append(&save_cur_btn);
        col1.append(&add_row);

        {
            let p_weak = Rc::downgrade(self);
            let ne = name_entry.clone();
            new_preset_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let mut st = PresetStore::load();
                    let raw = ne.text().trim().to_string();
                    let name = if raw.is_empty() {
                        format!("Preset {}", st.presets.len() + 1)
                    } else {
                        raw
                    };
                    let s = p.workspace.settings();
                    let preset = WorkspacePreset {
                        name: name.clone(),
                        run_on_startup: false,
                        window_opacity: Some(s.effective_terminal_bg_opacity()),
                        window_font_size: Some(s.effective_terminal_font_size()),
                        ui_font_size: Some(s.effective_ui_font_size()),
                        root: LayoutNode::Pane {
                            title: "Shell".to_string(),
                            pane_kind: PaneKind::Shell,
                            command: None,
                            working_dir: None,
                            opacity: None,
                            font_size: None,
                            show_scrollbar: None,
                        },
                    };
                    st.upsert(preset);
                    let _ = st.save();
                    if let Some(pos) = st.presets.iter().position(|pr| pr.name == name) {
                        p.selected_preset_index.set(Some(pos));
                    }
                    p.selected_preset_target.set(PresetEditorTarget::Window);
                    p.refresh_presets_ui();
                }
            });
        }
        {
            let p_weak = Rc::downgrade(self);
            let ne = name_entry.clone();
            save_cur_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let mut st = PresetStore::load();
                    let raw = ne.text().trim().to_string();
                    let name = if raw.is_empty() {
                        format!("Preset {}", st.presets.len() + 1)
                    } else {
                        raw
                    };
                    let preset = p.workspace.capture_current_preset(&name);
                    st.upsert(preset);
                    let _ = st.save();
                    if let Some(pos) = st.presets.iter().position(|pr| pr.name == name) {
                        p.selected_preset_index.set(Some(pos));
                    }
                    p.selected_preset_target.set(PresetEditorTarget::Window);
                    p.refresh_presets_ui();
                }
            });
        }

        if let Some(sel_idx) = selected_idx_opt
            && let Some(ref sel_pr) = selected_preset_opt
        {
            let act_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            let apply_col1_btn = gtk4::Button::with_label(&tr("Apply Now"));
            apply_col1_btn.set_hexpand(true);
            {
                let p_weak = Rc::downgrade(self);
                apply_col1_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let st = PresetStore::load();
                        if let Some(latest) = st.presets.get(sel_idx) {
                            p.workspace.apply_preset(latest);
                        }
                    }
                });
            }

            let del_col1_btn = gtk4::Button::with_label(&tr("Delete"));
            {
                let p_weak = Rc::downgrade(self);
                let preset_name = sel_pr.name.clone();
                del_col1_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = PresetStore::load();
                        st.remove_by_name(&preset_name);
                        let _ = st.save();
                        p.selected_preset_target.set(PresetEditorTarget::Window);
                        p.refresh_presets_ui();
                    }
                });
            }
            act_row.append(&apply_col1_btn);
            act_row.append(&del_col1_btn);
            col1.append(&act_row);
        }

        columns_row.append(&col1);

        // ====================================================================
        // COLUMN 2 (MIDDLE): Parent / Child Hierarchy Tree (Window -> Panel)
        // ====================================================================
        let col2 = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        col2.add_css_class("fwer-preset-col");
        col2.set_width_request(255);
        col2.set_vexpand(true);

        let col2_title = gtk4::Label::new(None);
        col2_title.set_markup(&format!("<b>2. {} (Window → Panel)</b>", tr("Hierarchy")));
        col2_title.set_halign(gtk4::Align::Start);
        col2.append(&col2_title);

        let col2_scroll = gtk4::ScrolledWindow::new();
        col2_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        col2_scroll.set_vexpand(true);
        col2_scroll.set_min_content_height(220);

        let tree_box = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        let pane_tree_labels: Rc<RefCell<HashMap<usize, gtk4::Label>>> =
            Rc::new(RefCell::new(HashMap::new()));

        if let Some(ref sel_pr) = selected_preset_opt {
            let is_win_sel = target == PresetEditorTarget::Window;
            let win_btn = gtk4::Button::new();
            win_btn.add_css_class("flat");
            win_btn.add_css_class(if is_win_sel {
                "fwer-preset-item-selected"
            } else {
                "fwer-preset-item"
            });

            let win_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            let win_lbl = gtk4::Label::new(None);
            win_lbl.set_markup(&format!(
                "▾ 🪟 <b>{}</b> <small>({})</small>",
                tr("Window"),
                glib::markup_escape_text(&sel_pr.name)
            ));
            win_lbl.set_halign(gtk4::Align::Start);
            win_lbl.set_hexpand(true);
            win_lbl.set_ellipsize(pango::EllipsizeMode::End);
            win_row.append(&win_lbl);

            if is_win_sel {
                let arrow = gtk4::Label::new(Some("→"));
                arrow.set_halign(gtk4::Align::End);
                win_row.append(&arrow);
            }
            win_btn.set_child(Some(&win_row));
            {
                let p_weak = Rc::downgrade(self);
                win_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        p.selected_preset_target.set(PresetEditorTarget::Window);
                        p.refresh_presets_ui();
                    }
                });
            }
            tree_box.append(&win_btn);

            let mut pane_counter = 0usize;
            self.append_preset_tree_node(
                &tree_box,
                &sel_pr.root,
                1,
                true,
                &mut pane_counter,
                target,
                &pane_tree_labels,
            );
        } else {
            let hint_lbl = gtk4::Label::new(Some(&tr("Select or create a preset on the left.")));
            hint_lbl.set_halign(gtk4::Align::Start);
            hint_lbl.set_wrap(true);
            tree_box.append(&hint_lbl);
        }

        col2_scroll.set_child(Some(&tree_box));
        col2.append(&col2_scroll);

        if let Some(sel_idx) = selected_idx_opt
            && let Some(ref sel_pr) = selected_preset_opt
        {
            col2.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
            let active_pane_idx = match target {
                PresetEditorTarget::Pane(i) => i,
                PresetEditorTarget::Window => 0,
            };
            let total_panes = sel_pr.root.pane_count();
            col2.append(&self.build_pane_structure_actions(
                sel_idx,
                active_pane_idx,
                total_panes,
                true,
            ));
        }

        columns_row.append(&col2);

        // ====================================================================
        // COLUMN 3 (RIGHT): Settings Inspector for Selected Window or Panel
        // ====================================================================
        let col3 = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        col3.add_css_class("fwer-preset-inspector");
        col3.set_hexpand(true);
        col3.set_vexpand(true);

        let Some(selected_idx) = selected_idx_opt else {
            let empty_inspector = gtk4::Label::new(Some(&tr("No Saved Presets")));
            empty_inspector.set_halign(gtk4::Align::Start);
            col3.append(&empty_inspector);
            columns_row.append(&col3);
            self.presets_box.append(&columns_row);
            return;
        };
        let Some(selected_preset) = selected_preset_opt else {
            columns_row.append(&col3);
            self.presets_box.append(&columns_row);
            return;
        };

        match target {
            PresetEditorTarget::Window => {
                let win_hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let win_sec_lbl = gtk4::Label::new(None);
                win_sec_lbl.set_markup(&format!(
                    "<b>3. 🪟 {} — {}</b>",
                    tr("Window Settings"),
                    glib::markup_escape_text(&selected_preset.name)
                ));
                win_sec_lbl.set_halign(gtk4::Align::Start);
                win_sec_lbl.set_hexpand(true);
                win_hdr.append(&win_sec_lbl);
                col3.append(&win_hdr);

                let hint_lbl = gtk4::Label::new(None);
                hint_lbl.set_markup(&format!(
                    "<small>{}: {}</small>",
                    tr("Split Structure"),
                    glib::markup_escape_text(&selected_preset.root.describe_structure())
                ));
                hint_lbl.set_halign(gtk4::Align::Start);
                col3.append(&hint_lbl);

                col3.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

                let rename_row = build_labeled_row(&tr("Preset Name"), 22);
                let rename_entry = gtk4::Entry::new();
                rename_entry.set_text(&selected_preset.name);
                rename_entry.set_hexpand(true);
                let rename_btn = gtk4::Button::with_label(&tr("Update Name"));
                {
                    let p_weak = Rc::downgrade(self);
                    let re = rename_entry.clone();
                    rename_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            let new_name = re.text().trim().to_string();
                            if !new_name.is_empty() {
                                mutate_preset(selected_idx, |pr| pr.name = new_name);
                                p.refresh_presets_ui();
                            }
                        }
                    });
                }
                {
                    let btn_c = rename_btn.clone();
                    rename_entry.connect_activate(move |_| {
                        btn_c.emit_clicked();
                    });
                }
                rename_row.append(&rename_entry);
                rename_row.append(&rename_btn);
                col3.append(&rename_row);

                let startup_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let startup_chk = gtk4::CheckButton::with_label(&tr("Run on Startup"));
                startup_chk.set_active(selected_preset.run_on_startup);
                startup_chk.set_hexpand(true);
                {
                    let p_weak = Rc::downgrade(self);
                    startup_chk.connect_toggled(move |chk| {
                        if let Some(p) = p_weak.upgrade() {
                            let mut st = PresetStore::load();
                            let target_name =
                                st.presets.get(selected_idx).map(|pr| pr.name.clone());
                            st.set_startup_preset(if chk.is_active() {
                                target_name.as_deref()
                            } else {
                                None
                            });
                            let _ = st.save();
                            p.refresh_presets_ui();
                        }
                    });
                }

                let overwrite_btn = gtk4::Button::with_label(&tr("Overwrite with Current"));
                {
                    let p_weak = Rc::downgrade(self);
                    overwrite_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            mutate_preset(selected_idx, |existing| {
                                let mut captured =
                                    p.workspace.capture_current_preset(&existing.name);
                                captured.run_on_startup = existing.run_on_startup;
                                *existing = captured;
                            });
                            p.refresh_presets_ui();
                        }
                    });
                }

                let apply_btn = gtk4::Button::with_label(&tr("Apply Now"));
                {
                    let p_weak = Rc::downgrade(self);
                    apply_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            let st = PresetStore::load();
                            if let Some(latest) = st.presets.get(selected_idx) {
                                p.workspace.apply_preset(latest);
                            }
                        }
                    });
                }

                startup_row.append(&startup_chk);
                startup_row.append(&overwrite_btn);
                startup_row.append(&apply_btn);
                col3.append(&startup_row);

                col3.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

                col3.append(&build_optional_scale_row(
                    &tr("Window Opacity"),
                    210,
                    selected_preset.window_opacity,
                    0.95,
                    0.0,
                    1.0,
                    0.02,
                    2,
                    false,
                    move |val| mutate_preset(selected_idx, |pr| pr.window_opacity = val),
                ));

                col3.append(&build_optional_scale_row(
                    &tr("Window Terminal Font Size (pt)"),
                    210,
                    selected_preset.window_font_size,
                    15.0,
                    8.0,
                    36.0,
                    0.5,
                    1,
                    true,
                    move |val| mutate_preset(selected_idx, |pr| pr.window_font_size = val),
                ));

                col3.append(&build_optional_scale_row(
                    &tr("Window UI Font Size (pt)"),
                    210,
                    selected_preset.ui_font_size,
                    15.0,
                    9.0,
                    28.0,
                    0.5,
                    1,
                    true,
                    move |val| mutate_preset(selected_idx, |pr| pr.ui_font_size = val),
                ));
            }
            PresetEditorTarget::Pane(pane_idx) => {
                let pane_cfgs = selected_preset.root.pane_configs();
                let total_panes = pane_cfgs.len();
                let cfg = pane_cfgs.get(pane_idx).cloned().unwrap_or_default();

                let hdr_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                let pane_lbl = gtk4::Label::new(None);
                pane_lbl.set_markup(&format!(
                    "<b>3. {} {} #{} — {}</b>",
                    cfg.pane_kind.icon(),
                    tr("Pane"),
                    pane_idx + 1,
                    tr("Panel Settings")
                ));
                pane_lbl.set_halign(gtk4::Align::Start);
                pane_lbl.set_hexpand(true);
                hdr_row.append(&pane_lbl);
                col3.append(&hdr_row);

                col3.append(&self.build_pane_structure_actions(
                    selected_idx,
                    pane_idx,
                    total_panes,
                    false,
                ));

                col3.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

                // Panel Type (Shell / Empty) row
                let kind_row = build_labeled_row(&tr("Panel Type"), 22);
                let kind_combo = gtk4::ComboBoxText::new();
                for &k in PaneKind::ALL {
                    kind_combo.append(
                        Some(k.id_str()),
                        &format!("{} {}", k.icon(), k.display_name()),
                    );
                }
                let _ = kind_combo.set_active_id(Some(cfg.pane_kind.id_str()));
                kind_combo.set_hexpand(true);
                disable_combo_scroll(&kind_combo);
                {
                    let p_weak = Rc::downgrade(self);
                    kind_combo.connect_changed(move |c| {
                        if let Some(id) = c.active_id()
                            && let Some(new_kind) = PaneKind::from_str_opt(&id)
                        {
                            mutate_preset_pane(selected_idx, pane_idx, |pane_cfg| {
                                pane_cfg.pane_kind = new_kind;
                                if new_kind == PaneKind::Empty
                                    && (pane_cfg.title == "/bin/bash" || pane_cfg.title == "Shell")
                                {
                                    pane_cfg.title = new_kind.default_title();
                                }
                            });
                            if let Some(p) = p_weak.upgrade() {
                                p.refresh_presets_ui();
                            }
                        }
                    });
                }
                kind_row.append(&kind_combo);
                col3.append(&kind_row);

                // Title row
                let title_row = build_labeled_row(&tr("Pane Title"), 22);
                let title_entry = gtk4::Entry::new();
                title_entry.set_text(&cfg.title);
                title_entry.set_hexpand(true);
                {
                    let labels_map = Rc::clone(&pane_tree_labels);
                    let cur_kind = cfg.pane_kind;
                    title_entry.connect_changed(move |e| {
                        let txt = e.text().to_string();
                        let effective_title = if txt.trim().is_empty() {
                            cur_kind.default_title()
                        } else {
                            txt
                        };
                        let title_for_save = effective_title.clone();
                        mutate_preset_pane(selected_idx, pane_idx, |c| {
                            c.title = title_for_save.clone();
                        });
                        if let Some(tree_lbl) = labels_map.borrow().get(&pane_idx) {
                            tree_lbl.set_markup(&format!(
                                "{} <b>{} #{}</b> <small>({})</small>",
                                cur_kind.icon(),
                                tr("Pane"),
                                pane_idx + 1,
                                glib::markup_escape_text(&effective_title)
                            ));
                        }
                    });
                }
                title_row.append(&title_entry);
                col3.append(&title_row);

                // Working dir row
                let wd_row = build_labeled_row(&tr("Working Directory"), 22);
                let wd_entry = gtk4::Entry::new();
                wd_entry.set_placeholder_text(Some("~/projects"));
                wd_entry.set_text(cfg.working_dir.as_deref().unwrap_or(""));
                wd_entry.set_hexpand(true);
                wd_entry.connect_changed(move |e| {
                    let txt = e.text().trim().to_string();
                    mutate_preset_pane(selected_idx, pane_idx, |c| {
                        c.working_dir = (!txt.is_empty()).then(|| txt.clone());
                    });
                });
                wd_row.append(&wd_entry);
                col3.append(&wd_row);

                // Command row
                let cmd_row = build_labeled_row(&tr("Auto-Run Command"), 22);
                let cmd_entry = gtk4::Entry::new();
                cmd_entry.set_placeholder_text(Some("e.g. nvim, htop, cargo test"));
                cmd_entry.set_text(cfg.command.as_deref().unwrap_or(""));
                cmd_entry.set_hexpand(true);
                cmd_entry.set_sensitive(cfg.pane_kind == PaneKind::Shell);
                cmd_entry.connect_changed(move |e| {
                    let txt = e.text().trim().to_string();
                    mutate_preset_pane(selected_idx, pane_idx, |c| {
                        c.command = (!txt.is_empty()).then(|| txt.clone());
                    });
                });
                cmd_row.append(&cmd_entry);
                col3.append(&cmd_row);

                // Panel Opacity row
                col3.append(&build_optional_scale_row(
                    &tr("Panel Opacity"),
                    198,
                    cfg.opacity,
                    0.95,
                    0.0,
                    1.0,
                    0.02,
                    2,
                    false,
                    move |val| mutate_preset_pane(selected_idx, pane_idx, |c| c.opacity = val),
                ));

                // Panel Font Size row
                col3.append(&build_optional_scale_row(
                    &tr("Panel Font Size (pt)"),
                    198,
                    cfg.font_size,
                    15.0,
                    8.0,
                    36.0,
                    0.5,
                    1,
                    true,
                    move |val| mutate_preset_pane(selected_idx, pane_idx, |c| c.font_size = val),
                ));

                // Panel Show Scrollbar row
                let default_sb = self.workspace.settings().show_scrollbar;
                let effective_sb = cfg.show_scrollbar.unwrap_or(default_sb);
                let p_sb_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let p_sb_chk = gtk4::CheckButton::with_label(&tr("Show Scrollbar"));
                p_sb_chk.set_active(effective_sb);
                p_sb_chk.set_hexpand(true);

                let p_sb_status = gtk4::Label::new(Some(&if cfg.show_scrollbar.is_some() {
                    tr("Applied")
                } else {
                    tr("Using Default")
                }));
                p_sb_status.add_css_class("dim-label");

                let p_sb_def_btn = gtk4::Button::with_label(&tr("Default"));
                p_sb_def_btn.set_sensitive(cfg.show_scrollbar.is_some());

                let sb_updating = Rc::new(Cell::new(false));
                {
                    let upd = Rc::clone(&sb_updating);
                    let st_lbl = p_sb_status.clone();
                    let def_btn = p_sb_def_btn.clone();
                    p_sb_chk.connect_toggled(move |chk| {
                        if upd.get() {
                            return;
                        }
                        let active = chk.is_active();
                        mutate_preset_pane(selected_idx, pane_idx, |c| {
                            c.show_scrollbar = Some(active);
                        });
                        st_lbl.set_text(&tr("Applied"));
                        def_btn.set_sensitive(true);
                    });
                }
                {
                    let upd = Rc::clone(&sb_updating);
                    let chk_c = p_sb_chk.clone();
                    let st_lbl = p_sb_status.clone();
                    let p_weak = Rc::downgrade(self);
                    p_sb_def_btn.connect_clicked(move |btn| {
                        let orig = p_weak
                            .upgrade()
                            .map(|p| p.workspace.settings().show_scrollbar)
                            .unwrap_or(default_sb);
                        upd.set(true);
                        chk_c.set_active(orig);
                        upd.set(false);
                        mutate_preset_pane(selected_idx, pane_idx, |c| {
                            c.show_scrollbar = None;
                        });
                        st_lbl.set_text(&tr("Using Default"));
                        btn.set_sensitive(false);
                    });
                }

                p_sb_row.append(&p_sb_chk);
                p_sb_row.append(&p_sb_status);
                p_sb_row.append(&p_sb_def_btn);
                col3.append(&p_sb_row);
            }
        }

        columns_row.append(&col3);
        self.presets_box.append(&columns_row);
    }

    #[allow(clippy::too_many_arguments)]
    fn append_preset_tree_node(
        self: &Rc<Self>,
        tree_box: &gtk4::Box,
        node: &LayoutNode,
        depth: usize,
        is_last: bool,
        pane_counter: &mut usize,
        selected_target: PresetEditorTarget,
        pane_tree_labels: &Rc<RefCell<HashMap<usize, gtk4::Label>>>,
    ) {
        let branch = if is_last { "└─" } else { "├─" };
        let indent_px = (depth as i32) * 14;

        match node {
            LayoutNode::Pane {
                title,
                pane_kind,
                command,
                ..
            } => {
                let p_idx = *pane_counter;
                *pane_counter += 1;

                let is_sel = selected_target == PresetEditorTarget::Pane(p_idx);
                let pane_btn = gtk4::Button::new();
                pane_btn.add_css_class("flat");
                pane_btn.add_css_class(if is_sel {
                    "fwer-preset-item-selected"
                } else {
                    "fwer-preset-item"
                });
                pane_btn.set_margin_start(indent_px);

                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
                let branch_lbl = gtk4::Label::new(Some(branch));
                branch_lbl.add_css_class("fwer-preset-group-node");
                row.append(&branch_lbl);

                let cmd_suffix = command
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .map(|c| format!(" [{}]", glib::markup_escape_text(c)))
                    .unwrap_or_default();

                let lbl = gtk4::Label::new(None);
                lbl.set_markup(&format!(
                    "{} <b>{} #{}</b> <small>({}{})</small>",
                    pane_kind.icon(),
                    tr("Pane"),
                    p_idx + 1,
                    glib::markup_escape_text(title),
                    cmd_suffix
                ));
                lbl.set_halign(gtk4::Align::Start);
                lbl.set_hexpand(true);
                lbl.set_ellipsize(pango::EllipsizeMode::End);
                row.append(&lbl);

                pane_tree_labels.borrow_mut().insert(p_idx, lbl);

                if is_sel {
                    let arrow = gtk4::Label::new(Some("→"));
                    arrow.set_halign(gtk4::Align::End);
                    row.append(&arrow);
                }

                pane_btn.set_child(Some(&row));
                {
                    let p_weak = Rc::downgrade(self);
                    pane_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            p.selected_preset_target
                                .set(PresetEditorTarget::Pane(p_idx));
                            p.refresh_presets_ui();
                        }
                    });
                }
                tree_box.append(&pane_btn);
            }
            LayoutNode::HorizontalSplit { children }
            | LayoutNode::VerticalSplit { children }
            | LayoutNode::Tabs { children } => {
                let group_label = match node {
                    LayoutNode::HorizontalSplit { .. } => {
                        format!("{} ▾ ↔ {}", branch, tr("Horizontal Split"))
                    }
                    LayoutNode::VerticalSplit { .. } => {
                        format!("{} ▾ ↕ {}", branch, tr("Vertical Split"))
                    }
                    LayoutNode::Tabs { .. } => {
                        format!("{} ▾ 📑 {}", branch, tr("Tabs"))
                    }
                    _ => String::new(),
                };

                let grp_lbl = gtk4::Label::new(None);
                grp_lbl.set_markup(&format!(
                    "<small><b>{}</b></small>",
                    glib::markup_escape_text(&group_label)
                ));
                grp_lbl.add_css_class("fwer-preset-group-node");
                grp_lbl.set_halign(gtk4::Align::Start);
                grp_lbl.set_margin_start(indent_px);
                tree_box.append(&grp_lbl);

                let len = children.len();
                for (i, child) in children.iter().enumerate() {
                    self.append_preset_tree_node(
                        tree_box,
                        child,
                        depth + 1,
                        i + 1 == len,
                        pane_counter,
                        selected_target,
                        pane_tree_labels,
                    );
                }
            }
        }
    }
}
