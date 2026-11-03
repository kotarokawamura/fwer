// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

#![allow(deprecated)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;

use crate::config::layout_preset::{LayoutNode, PresetStore, WorkspacePreset};
use crate::config::theme::{
    ShortcutSettings, ThemeSettings, cached_system_fonts, extract_font_family_from_path,
    format_recorded_shortcut, format_rgb_hex, is_font_cjk, is_font_monospace, is_valid_font_file,
    matches_gdk_shortcut, parse_color, split_shortcut_bindings,
};
use crate::i18n::{
    active_entry_count, active_locale, discover_po_files, load_catalog_for_language, tr,
};
use crate::ui::workspace::Workspace;
use crate::vte::RgbColor;

const MAX_COMBO_FONTS: usize = 200;

#[derive(Debug, Clone)]
struct RecordingTarget {
    slot_key: String,
    append: bool,
}

#[derive(Clone)]
struct ShortcutRowWidgets {
    entry: gtk4::Entry,
    chips_box: gtk4::Box,
    updating: Rc<Cell<bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresetEditorTarget {
    Window,
    Pane(usize),
}

pub struct PreferencesWindow {
    pub window: gtk4::Window,
    workspace: Rc<Workspace>,
    stack: gtk4::Stack,
    body_revealer: gtk4::Revealer,
    collapsed: Cell<bool>,
    save_pending: Cell<bool>,
    recording: Rc<RefCell<Option<RecordingTarget>>>,
    recording_status_label: gtk4::Label,
    shortcut_rows: RefCell<HashMap<String, ShortcutRowWidgets>>,
    discovered_fonts: Rc<RefCell<Vec<PathBuf>>>,
    fonts_loaded: Cell<bool>,
    font_tab_populated: Cell<bool>,
    selected_preset_index: Cell<Option<usize>>,
    selected_preset_target: Cell<PresetEditorTarget>,
    presets_box: gtk4::Box,
    catalog_window: RefCell<Option<gtk4::Window>>,
    on_settings_applied: Rc<dyn Fn(&ThemeSettings)>,
}

impl PreferencesWindow {
    pub fn new(
        parent: &impl IsA<gtk4::Window>,
        workspace: Rc<Workspace>,
        on_settings_applied: impl Fn(&ThemeSettings) + 'static,
    ) -> Rc<Self> {
        let window = gtk4::Window::builder()
            .title(tr("Termin Preferences"))
            .transient_for(parent)
            .default_width(920)
            .default_height(560)
            .width_request(720)
            .height_request(420)
            .resizable(true)
            .build();

        window.add_css_class("fwer-prefs-window");
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        stack.set_hexpand(true);
        stack.set_vexpand(true);

        let body_revealer = gtk4::Revealer::new();
        body_revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        body_revealer.set_reveal_child(true);
        body_revealer.set_hexpand(true);
        body_revealer.set_vexpand(true);

        let recording = Rc::new(RefCell::new(None));
        let recording_status_label = gtk4::Label::new(None);
        recording_status_label.add_css_class("fwer-record-banner");
        recording_status_label.set_visible(false);

        let discovered_fonts = Rc::new(RefCell::new(Vec::new()));
        let presets_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        presets_box.set_hexpand(true);
        presets_box.set_vexpand(true);

        let prefs = Rc::new(Self {
            window,
            workspace,
            stack,
            body_revealer,
            collapsed: Cell::new(false),
            save_pending: Cell::new(false),
            recording,
            recording_status_label,
            shortcut_rows: RefCell::new(HashMap::new()),
            discovered_fonts,
            fonts_loaded: Cell::new(false),
            font_tab_populated: Cell::new(false),
            selected_preset_index: Cell::new(Some(0)),
            selected_preset_target: Cell::new(PresetEditorTarget::Window),
            presets_box,
            catalog_window: RefCell::new(None),
            on_settings_applied: Rc::new(on_settings_applied),
        });

        prefs.build_ui();
        prefs.setup_window_controllers();
        prefs
    }

    fn ensure_fonts_loaded(&self) {
        if !self.fonts_loaded.get() {
            *self.discovered_fonts.borrow_mut() = cached_system_fonts(false);
            self.fonts_loaded.set(true);
        }
    }

    pub fn present(self: &Rc<Self>) {
        self.window.set_title(Some(&tr("Termin Preferences")));
        if self.stack.first_child().is_none() {
            self.rebuild_tabs();
        } else {
            self.refresh_presets_ui();
        }
        self.window.present();
    }

    pub fn toggle_collapse(&self) {
        let next = !self.collapsed.get();
        self.collapsed.set(next);
        self.body_revealer.set_reveal_child(!next);
        if next {
            self.window.set_default_size(920, 56);
        } else {
            self.window.set_default_size(920, 560);
        }
    }

    fn notify_changed(self: &Rc<Self>, settings: ThemeSettings) {
        let mut sanitized = settings;
        sanitized.sanitize();
        if self.workspace.settings() == sanitized {
            return;
        }
        self.workspace.apply_settings(sanitized.clone());
        (self.on_settings_applied)(&sanitized);

        if !self.save_pending.get() {
            self.save_pending.set(true);
            let p_weak = Rc::downgrade(self);
            glib::timeout_add_local_once(Duration::from_millis(200), move || {
                if let Some(p) = p_weak.upgrade() {
                    p.save_pending.set(false);
                    let _ = p.workspace.settings().save();
                }
            });
        }
    }

    fn build_ui(self: &Rc<Self>) {
        let outer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        outer.set_hexpand(true);
        outer.set_vexpand(true);

        let header_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header_bar.set_margin_top(8);
        header_bar.set_margin_bottom(8);
        header_bar.set_margin_start(10);
        header_bar.set_margin_end(10);

        let tabs = [
            ("titlebar", tr("Titlebar & Frame")),
            ("font", tr("Font")),
            ("shortcuts", tr("Shortcuts")),
            ("presets", tr("Presets")),
            ("language", tr("Language")),
        ];

        for (id, title) in tabs {
            let btn = gtk4::Button::with_label(&title);
            let p_weak = Rc::downgrade(self);
            let tab_id = id.to_string();
            btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    if tab_id == "font" && !p.font_tab_populated.get() {
                        p.populate_font_tab_now();
                    }
                    if tab_id == "presets" {
                        p.refresh_presets_ui();
                    }
                    p.stack.set_visible_child_name(&tab_id);
                }
            });
            header_bar.append(&btn);
        }

        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header_bar.append(&spacer);

        let collapse_btn = gtk4::Button::with_label("▾");
        collapse_btn.set_tooltip_text(Some("Collapse / Expand (Ctrl+Shift+M)"));
        {
            let p_weak = Rc::downgrade(self);
            collapse_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    p.toggle_collapse();
                }
            });
        }
        header_bar.append(&collapse_btn);

        outer.append(&header_bar);
        outer.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        outer.append(&self.recording_status_label);

        let body_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body_box.set_hexpand(true);
        body_box.set_vexpand(true);

        body_box.append(&self.stack);
        body_box.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        footer.set_margin_top(8);
        footer.set_margin_bottom(8);
        footer.set_margin_start(12);
        footer.set_margin_end(12);

        let restore_btn = gtk4::Button::with_label(&tr("Restore Defaults"));
        {
            let p_weak = Rc::downgrade(self);
            restore_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let defaults = ThemeSettings::default();
                    load_catalog_for_language(&defaults.language, &defaults.custom_po_path);
                    p.notify_changed(defaults);
                    p.rebuild_tabs();
                }
            });
        }

        let save_layout_btn = gtk4::Button::with_label(&tr("Save Current Layout"));
        {
            let p_weak = Rc::downgrade(self);
            save_layout_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let _ = p.workspace.save_current_layout();
                }
            });
        }

        let load_layout_btn = gtk4::Button::with_label(&tr("Load Saved Layout"));
        {
            let p_weak = Rc::downgrade(self);
            load_layout_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let _ = p.workspace.load_saved_layout();
                }
            });
        }

        let footer_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        footer_spacer.set_hexpand(true);

        let close_btn = gtk4::Button::with_label(&tr("Close"));
        {
            let win_clone = self.window.clone();
            close_btn.connect_clicked(move |_| {
                win_clone.set_visible(false);
            });
        }

        footer.append(&restore_btn);
        footer.append(&save_layout_btn);
        footer.append(&load_layout_btn);
        footer.append(&footer_spacer);
        footer.append(&close_btn);

        body_box.append(&footer);
        self.body_revealer.set_child(Some(&body_box));
        outer.append(&self.body_revealer);

        self.window.set_child(Some(&outer));
        self.rebuild_tabs();
    }

    fn setup_window_controllers(self: &Rc<Self>) {
        {
            let ws_clone = Rc::clone(&self.workspace);
            self.window.connect_close_request(move |win| {
                ws_clone.set_modal_blocked(false);
                win.set_visible(false);
                glib::Propagation::Stop
            });
        }
        {
            let ws_clone = Rc::clone(&self.workspace);
            self.window
                .connect_notify_local(Some("is-active"), move |win, _| {
                    ws_clone.set_modal_blocked(win.is_active());
                });
        }

        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let p_weak = Rc::downgrade(self);
        key_ctrl.connect_key_pressed(move |_ctrl, key, _code, mods| {
            let Some(p) = p_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };

            let rec_target = p.recording.borrow().clone();
            if let Some(target) = rec_target {
                if key == gdk::Key::Escape && mods.is_empty() {
                    *p.recording.borrow_mut() = None;
                    p.recording_status_label.set_visible(false);
                    return glib::Propagation::Stop;
                }
                if let Some(formatted) = format_recorded_shortcut(key, mods) {
                    *p.recording.borrow_mut() = None;
                    p.recording_status_label.set_visible(false);
                    let mut settings = p.workspace.settings();
                    apply_shortcut_slot_update(
                        &mut settings.shortcuts,
                        &target.slot_key,
                        &formatted,
                        target.append,
                    );
                    p.notify_changed(settings);
                    // タブを再構築せず、該当行のUIだけを直接更新（スクロール位置が飛ばない）
                    p.refresh_shortcut_row_ui(&target.slot_key);
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Stop;
            }

            let sc = p.workspace.settings().shortcuts;
            if matches_gdk_shortcut(&sc.close_preferences, key, mods) {
                p.workspace.set_modal_blocked(false);
                p.window.set_visible(false);
                return glib::Propagation::Stop;
            }
            if matches_gdk_shortcut(&sc.toggle_collapse_preferences, key, mods) {
                p.toggle_collapse();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(key_ctrl);
    }

    fn wrap_tab_scrolled(child: &impl IsA<gtk4::Widget>) -> gtk4::ScrolledWindow {
        gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .child(child)
            .build()
    }

    fn rebuild_tabs(self: &Rc<Self>) {
        let cur_tab = self
            .stack
            .visible_child_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "titlebar".to_string());

        if let Some(vp) = self
            .presets_box
            .parent()
            .and_then(|p| p.downcast::<gtk4::Viewport>().ok())
        {
            vp.set_child(gtk4::Widget::NONE);
        }

        while let Some(child) = self.stack.first_child() {
            self.stack.remove(&child);
        }

        let tab1 = Self::wrap_tab_scrolled(&self.build_titlebar_and_frame_tab());
        let tab2: gtk4::Widget = if cur_tab == "font" {
            self.font_tab_populated.set(true);
            Self::wrap_tab_scrolled(&self.build_font_tab()).upcast()
        } else {
            self.font_tab_populated.set(false);
            gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast()
        };
        let tab3 = Self::wrap_tab_scrolled(&self.build_shortcuts_tab());
        self.refresh_presets_ui();
        let tab_presets = Self::wrap_tab_scrolled(&self.presets_box);
        let tab4 = Self::wrap_tab_scrolled(&self.build_language_tab());

        self.stack.add_named(&tab1, Some("titlebar"));
        self.stack.add_named(&tab2, Some("font"));
        self.stack.add_named(&tab3, Some("shortcuts"));
        self.stack.add_named(&tab_presets, Some("presets"));
        self.stack.add_named(&tab4, Some("language"));

        self.stack.set_visible_child_name(&cur_tab);
    }

    fn populate_font_tab_now(self: &Rc<Self>) {
        if let Some(old_font_tab) = self.stack.child_by_name("font") {
            self.stack.remove(&old_font_tab);
        }
        let tab2 = Self::wrap_tab_scrolled(&self.build_font_tab());
        self.stack.add_named(&tab2, Some("font"));
        self.font_tab_populated.set(true);
    }

    fn refresh_shortcut_row_ui(self: &Rc<Self>, slot_key: &str) {
        let val = get_shortcut_slot_value(&self.workspace.settings().shortcuts, slot_key);
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

    fn refresh_all_shortcut_rows_ui(self: &Rc<Self>) {
        let keys: Vec<String> = self.shortcut_rows.borrow().keys().cloned().collect();
        for k in keys {
            self.refresh_shortcut_row_ui(&k);
        }
    }

    fn rebuild_row_chips(self: &Rc<Self>, chips_box: &gtk4::Box, slot_key: &str, current_val: &str) {
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
                        apply_shortcut_slot_update(&mut st.shortcuts, &key_owned, &joined, false);
                        p.notify_changed(st);
                        p.refresh_shortcut_row_ui(&key_owned);
                    }
                });
                chips_box.append(&chip_btn);
            }
        }
    }

    fn build_titlebar_and_frame_tab(self: &Rc<Self>) -> gtk4::Widget {
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        let s = self.workspace.settings();

        self.add_color_editor_row(
            &vbox,
            &tr("Active Titlebar Background"),
            &s.title_active_bg,
            RgbColor(117, 33, 0),
            |st, v| st.title_active_bg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Active Titlebar Text"),
            &s.title_active_fg,
            RgbColor(255, 255, 255),
            |st, v| st.title_active_fg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Inactive Titlebar Background"),
            &s.title_inactive_bg,
            RgbColor(75, 75, 75),
            |st, v| st.title_inactive_bg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Inactive Titlebar Text"),
            &s.title_inactive_fg,
            RgbColor(210, 210, 210),
            |st, v| st.title_inactive_fg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Window Frame Background"),
            &s.frame_bg_color,
            RgbColor(43, 43, 43),
            |st, v| st.frame_bg_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Window Frame Border"),
            &s.frame_border_color,
            RgbColor(90, 90, 90),
            |st, v| st.frame_border_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Pane Separator Color"),
            &s.separator_color,
            RgbColor(74, 74, 74),
            |st, v| st.separator_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Pane Separator Border Color"),
            &s.separator_border_color,
            s.effective_separator_rgb(),
            |st, v| st.separator_border_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Terminal Background Color"),
            &s.terminal_bg_color,
            RgbColor(0, 0, 0),
            |st, v| st.terminal_bg_color = v,
        );

        vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        self.add_slider_row(
            &vbox,
            &tr("Terminal Background Opacity"),
            s.effective_terminal_bg_opacity() as f64,
            0.0,
            1.0,
            0.05,
            2,
            |st, v| st.terminal_bg_opacity = v as f32,
        );
        self.add_slider_row(
            &vbox,
            &tr("Pane Separator Width (px)"),
            s.effective_separator_width() as f64,
            1.0,
            16.0,
            1.0,
            0,
            |st, v| st.separator_width = v as f32,
        );
        self.add_slider_row(
            &vbox,
            &tr("Window Border Width (px)"),
            s.effective_border_width() as f64,
            1.0,
            20.0,
            1.0,
            0,
            |st, v| st.frame_border_width = v as f32,
        );
        let fps_enabled = self.workspace.is_fps_control_enabled();
        self.add_slider_row_with_enabled(
            &vbox,
            &tr("Target FPS"),
            s.effective_target_fps() as f64,
            1.0,
            240.0,
            5.0,
            0,
            fps_enabled,
            |st, v| st.target_fps = v.round() as u32,
        );

        let chk = gtk4::CheckButton::with_label(&tr("Show Scrollbar"));
        chk.set_active(s.show_scrollbar);
        {
            let p_weak = Rc::downgrade(self);
            chk.connect_toggled(move |c| {
                if let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    st.show_scrollbar = c.is_active();
                    p.notify_changed(st);
                }
            });
        }
        vbox.append(&chk);

        vbox.upcast()
    }

    fn add_color_editor_row<F>(
        self: &Rc<Self>,
        container: &gtk4::Box,
        label_text: &str,
        initial_val: &str,
        fallback: RgbColor,
        setter: F,
    ) where
        F: Fn(&mut ThemeSettings, String) + 'static,
    {
        let setter = Rc::new(setter);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

        let lbl = gtk4::Label::new(Some(label_text));
        lbl.set_width_chars(26);
        lbl.set_halign(gtk4::Align::Start);
        row.append(&lbl);

        let entry = gtk4::Entry::new();
        entry.set_text(initial_val);
        entry.set_placeholder_text(Some(&format_rgb_hex(fallback)));
        entry.set_width_chars(14);
        row.append(&entry);

        let color_dialog = gtk4::ColorDialog::new();
        color_dialog.set_with_alpha(false);
        let eff_rgb = parse_color(initial_val).unwrap_or(fallback);
        let color_btn = gtk4::ColorDialogButton::new(Some(color_dialog));
        color_btn.set_rgba(&gdk::RGBA::new(
            eff_rgb.0 as f32 / 255.0,
            eff_rgb.1 as f32 / 255.0,
            eff_rgb.2 as f32 / 255.0,
            1.0,
        ));
        row.append(&color_btn);

        let status_lbl = gtk4::Label::new(Some(&color_status_text(initial_val)));
        status_lbl.set_width_chars(18);
        status_lbl.set_halign(gtk4::Align::Start);
        row.append(&status_lbl);

        let def_btn = gtk4::Button::with_label(&tr("Default"));
        row.append(&def_btn);

        let updating = Rc::new(Cell::new(false));

        {
            let p_weak = Rc::downgrade(self);
            let setter_clone = Rc::clone(&setter);
            let status_clone = status_lbl.clone();
            let color_btn_clone = color_btn.clone();
            let upd = Rc::clone(&updating);
            entry.connect_changed(move |e| {
                if upd.get() {
                    return;
                }
                let txt = e.text().to_string();
                status_clone.set_text(&color_status_text(&txt));
                let parsed = parse_color(&txt);
                let rgb = parsed.unwrap_or(fallback);
                upd.set(true);
                color_btn_clone.set_rgba(&gdk::RGBA::new(
                    rgb.0 as f32 / 255.0,
                    rgb.1 as f32 / 255.0,
                    rgb.2 as f32 / 255.0,
                    1.0,
                ));
                upd.set(false);
                if (txt.trim().is_empty() || parsed.is_some())
                    && let Some(p) = p_weak.upgrade()
                {
                    let mut st = p.workspace.settings();
                    setter_clone(&mut st, txt);
                    p.notify_changed(st);
                }
            });
        }

        {
            let entry_clone = entry.clone();
            let status_clone = status_lbl.clone();
            let upd = Rc::clone(&updating);
            let p_weak = Rc::downgrade(self);
            let setter_clone = Rc::clone(&setter);

            color_btn.connect_rgba_notify(move |btn| {
                if upd.get() {
                    return;
                }
                let rgba = btn.rgba();
                let r = (rgba.red() * 255.0).round().clamp(0.0, 255.0) as u8;
                let g = (rgba.green() * 255.0).round().clamp(0.0, 255.0) as u8;
                let b = (rgba.blue() * 255.0).round().clamp(0.0, 255.0) as u8;
                let hex = format_rgb_hex(RgbColor(r, g, b));

                upd.set(true);
                entry_clone.set_text(&hex);
                upd.set(false);

                status_clone.set_text(&color_status_text(&hex));

                if let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    setter_clone(&mut st, hex);
                    p.notify_changed(st);
                }
            });
        }

        {
            let entry_clone = entry.clone();
            def_btn.connect_clicked(move |_| {
                entry_clone.set_text("");
            });
        }

        container.append(&row);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_slider_row<F>(
        self: &Rc<Self>,
        container: &gtk4::Box,
        label_text: &str,
        initial_val: f64,
        min: f64,
        max: f64,
        step: f64,
        digits: i32,
        setter: F,
    ) -> gtk4::Box
    where
        F: Fn(&mut ThemeSettings, f64) + 'static,
    {
        self.add_slider_row_with_enabled(
            container,
            label_text,
            initial_val,
            min,
            max,
            step,
            digits,
            true,
            setter,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add_slider_row_with_enabled<F>(
        self: &Rc<Self>,
        container: &gtk4::Box,
        label_text: &str,
        initial_val: f64,
        min: f64,
        max: f64,
        step: f64,
        digits: i32,
        enabled: bool,
        setter: F,
    ) -> gtk4::Box
    where
        F: Fn(&mut ThemeSettings, f64) + 'static,
    {
        let setter = Rc::new(setter);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

        let lbl = gtk4::Label::new(Some(label_text));
        lbl.set_width_chars(26);
        lbl.set_halign(gtk4::Align::Start);
        row.append(&lbl);

        let minus_btn = gtk4::Button::with_label("−");
        row.append(&minus_btn);

        let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, min, max, step);
        scale.set_digits(digits);
        scale.set_draw_value(true);
        scale.set_value(initial_val);
        scale.set_hexpand(true);
        row.append(&scale);

        let plus_btn = gtk4::Button::with_label("+");
        row.append(&plus_btn);

        if enabled {
            {
                let sc_clone = scale.clone();
                minus_btn.connect_clicked(move |_| {
                    sc_clone.set_value((sc_clone.value() - step).clamp(min, max));
                });
            }
            {
                let sc_clone = scale.clone();
                plus_btn.connect_clicked(move |_| {
                    sc_clone.set_value((sc_clone.value() + step).clamp(min, max));
                });
            }
            {
                let p_weak = Rc::downgrade(self);
                let setter_clone = Rc::clone(&setter);
                scale.connect_value_changed(move |sc| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = p.workspace.settings();
                        setter_clone(&mut st, sc.value());
                        p.notify_changed(st);
                    }
                });
            }
        } else {
            row.add_css_class("fwer-disabled-row");
            row.set_sensitive(false);
            row.set_can_target(false);
            lbl.set_sensitive(false);
            minus_btn.set_sensitive(false);
            minus_btn.set_can_target(false);
            minus_btn.set_focusable(false);
            scale.set_sensitive(false);
            scale.set_can_target(false);
            scale.set_focusable(false);
            plus_btn.set_sensitive(false);
            plus_btn.set_can_target(false);
            plus_btn.set_focusable(false);

            let disabled_lbl = gtk4::Label::new(Some(&tr("Disabled (--fps not specified)")));
            disabled_lbl.set_sensitive(false);
            row.append(&disabled_lbl);
            row.set_tooltip_text(Some(&tr("Disabled (--fps not specified)")));
        }

        container.append(&row);
        row
    }

    fn build_font_tab(self: &Rc<Self>) -> gtk4::Widget {
        self.ensure_fonts_loaded();

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        let s = self.workspace.settings();

        let filter_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let filter_lbl = gtk4::Label::new(Some(&tr("Font Filter Keyword")));
        filter_lbl.set_width_chars(22);
        filter_lbl.set_halign(gtk4::Align::Start);
        let filter_entry = gtk4::Entry::new();
        filter_entry.set_hexpand(true);
        let rescan_btn = gtk4::Button::with_label(&tr("Rescan"));
        filter_row.append(&filter_lbl);
        filter_row.append(&filter_entry);
        filter_row.append(&rescan_btn);
        vbox.append(&filter_row);

        let catalog_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let catalog_spacer = gtk4::Label::new(None);
        catalog_spacer.set_width_chars(22);
        let catalog_btn =
            gtk4::Button::with_label(&format!("🗚 {}", tr("Open Font Sample Catalog...")));
        catalog_btn.set_hexpand(true);
        catalog_row.append(&catalog_spacer);
        catalog_row.append(&catalog_btn);
        vbox.append(&catalog_row);

        let combo_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let combo_lbl = gtk4::Label::new(Some(&tr("System Font")));
        combo_lbl.set_width_chars(22);
        combo_lbl.set_halign(gtk4::Align::Start);
        #[allow(deprecated)]
        let font_combo = gtk4::ComboBoxText::new();
        font_combo.set_hexpand(true);
        // コンボボックス上でスワイプ（ホイールスクロール）しても選択フォントが変わらないようにブロック
        disable_combo_scroll(&font_combo);
        combo_row.append(&combo_lbl);
        combo_row.append(&font_combo);
        vbox.append(&combo_row);

        let path_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let path_lbl = gtk4::Label::new(Some(&tr("Font File Path")));
        path_lbl.set_width_chars(22);
        path_lbl.set_halign(gtk4::Align::Start);
        let path_entry = gtk4::Entry::new();
        path_entry.set_text(&s.font_path);
        path_entry.set_hexpand(true);
        path_row.append(&path_lbl);
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

        // コンボボックスへの追加を最大 MAX_COMBO_FONTS (200件) に制限して高速化
        let populate_combo = {
            let fonts_rc = Rc::clone(&self.discovered_fonts);
            let combo_clone = font_combo.clone();
            let pop_flag = Rc::clone(&combo_populating);
            move |kw: &str, current_path: &str| {
                pop_flag.set(true);
                #[allow(deprecated)]
                combo_clone.remove_all();
                let kw_lower = kw.trim().to_ascii_lowercase();
                let mut count = 0usize;
                let mut current_added = current_path.trim().is_empty();

                for path in fonts_rc.borrow().iter() {
                    let p_str = path.to_string_lossy();
                    if !kw_lower.is_empty() && !p_str.to_ascii_lowercase().contains(&kw_lower) {
                        continue;
                    }
                    let label = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&p_str);
                    #[allow(deprecated)]
                    combo_clone.append(Some(&p_str), label);
                    if p_str == current_path {
                        current_added = true;
                    }
                    count += 1;
                    if count >= MAX_COMBO_FONTS {
                        break;
                    }
                }

                // 現在選択中のフォントが上限200件の外にある場合でもリストに追加して選択状態を維持する
                if !current_added && Path::new(current_path).exists() {
                    let label = Path::new(current_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(current_path);
                    #[allow(deprecated)]
                    combo_clone.append(Some(current_path), label);
                }

                if !current_path.trim().is_empty() {
                    #[allow(deprecated)]
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
                #[allow(deprecated)]
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

    fn open_font_sample_catalog(self: &Rc<Self>, path_entry_to_sync: gtk4::Entry) {
        self.ensure_fonts_loaded();

        if let Some(existing) = self.catalog_window.borrow().as_ref() {
            existing.present();
            return;
        }

        let cat_win = gtk4::Window::builder()
            .title(tr("System Font Samples & Selector"))
            .transient_for(&self.window)
            .default_width(760)
            .default_height(540)
            .width_request(560)
            .height_request(380)
            .build();

        cat_win.add_css_class("fwer-prefs-window");
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        root.set_margin_top(10);
        root.set_margin_bottom(10);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let desc_lbl = gtk4::Label::new(Some(&tr(
            "Browse installed system fonts with live text samples and click 'Use This Font' to apply.",
        )));
        desc_lbl.set_halign(gtk4::Align::Start);
        desc_lbl.add_css_class("dim-label");
        root.append(&desc_lbl);

        let cat_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let cat_lbl = gtk4::Label::new(Some(&tr("Category:")));
        cat_lbl.set_width_chars(18);
        cat_lbl.set_halign(gtk4::Align::Start);
        let btn_all = gtk4::ToggleButton::with_label(&tr("All Fonts"));
        let btn_mono = gtk4::ToggleButton::with_label(&tr("Monospace"));
        let btn_cjk = gtk4::ToggleButton::with_label(&tr("Japanese / CJK"));
        btn_mono.set_group(Some(&btn_all));
        btn_cjk.set_group(Some(&btn_all));
        btn_all.set_active(true);

        cat_row.append(&cat_lbl);
        cat_row.append(&btn_all);
        cat_row.append(&btn_mono);
        cat_row.append(&btn_cjk);
        root.append(&cat_row);

        let filter_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let filter_lbl = gtk4::Label::new(Some(&tr("Font Filter Keyword")));
        filter_lbl.set_width_chars(18);
        filter_lbl.set_halign(gtk4::Align::Start);
        let search_entry = gtk4::Entry::new();
        search_entry.set_placeholder_text(Some("noto, mono, vl, gothic..."));
        search_entry.set_hexpand(true);
        let rescan_btn = gtk4::Button::with_label(&tr("Rescan"));
        filter_row.append(&filter_lbl);
        filter_row.append(&search_entry);
        filter_row.append(&rescan_btn);
        root.append(&filter_row);

        let sample_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let sample_lbl = gtk4::Label::new(Some(&tr("Sample Text")));
        sample_lbl.set_width_chars(18);
        sample_lbl.set_halign(gtk4::Align::Start);
        let sample_ascii_entry = gtk4::Entry::new();
        sample_ascii_entry.set_text(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789 !@#$%^&*()",
        );
        sample_ascii_entry.set_hexpand(true);
        let sample_cjk_entry = gtk4::Entry::new();
        sample_cjk_entry.set_text(
            "日本語サンプル: 吾輩は猫である。名前はまだ無い。 (漢字・ひらがな・カタカナ)",
        );
        sample_cjk_entry.set_hexpand(true);
        sample_row.append(&sample_lbl);
        sample_row.append(&sample_ascii_entry);
        sample_row.append(&sample_cjk_entry);
        root.append(&sample_row);

        root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let matching_lbl = gtk4::Label::new(None);
        matching_lbl.set_halign(gtk4::Align::Start);
        root.append(&matching_lbl);

        let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let scrolled = gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&list_box)
            .build();
        root.append(&scrolled);

        root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let current_font_lbl = gtk4::Label::new(Some(&format!(
            "{}: {}",
            tr("Current Font"),
            self.workspace.settings().font_path
        )));
        current_font_lbl.set_halign(gtk4::Align::Start);
        current_font_lbl.set_hexpand(true);
        current_font_lbl.set_ellipsize(pango::EllipsizeMode::Middle);
        let close_btn = gtk4::Button::with_label(&tr("Close"));
        footer.append(&current_font_lbl);
        footer.append(&close_btn);
        root.append(&footer);

        let live_ascii_labels: Rc<RefCell<Vec<gtk4::Label>>> = Rc::new(RefCell::new(Vec::new()));
        let live_cjk_labels: Rc<RefCell<Vec<gtk4::Label>>> = Rc::new(RefCell::new(Vec::new()));

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
                current_font_lbl_c
                    .set_text(&format!("{}: {}", tr("Current Font"), current_selected));

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

                    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                    card.add_css_class(if is_selected {
                        "fwer-font-card-selected"
                    } else {
                        "fwer-font-card"
                    });

                    let header_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                    let name_lbl = gtk4::Label::new(None);
                    name_lbl.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(font_name)));
                    name_lbl.set_halign(gtk4::Align::Start);
                    header_row.append(&name_lbl);

                    if is_font_monospace(path) {
                        let mono_badge = gtk4::Label::new(None);
                        mono_badge.set_markup("<span foreground=\"#60a5fa\"><b>[Mono]</b></span>");
                        header_row.append(&mono_badge);
                    }
                    if is_font_cjk(path) {
                        let cjk_badge = gtk4::Label::new(None);
                        cjk_badge.set_markup("<span foreground=\"#facc15\"><b>[CJK]</b></span>");
                        header_row.append(&cjk_badge);
                    }

                    let path_dim = gtk4::Label::new(Some(&path_str));
                    path_dim.add_css_class("dim-label");
                    path_dim.set_halign(gtk4::Align::Start);
                    path_dim.set_hexpand(true);
                    path_dim.set_ellipsize(pango::EllipsizeMode::Middle);
                    header_row.append(&path_dim);

                    if is_selected {
                        let sel_lbl = gtk4::Label::new(None);
                        sel_lbl.set_markup(&format!(
                            "<span foreground=\"#4ade80\"><b>✔ {}</b></span>",
                            glib::markup_escape_text(&tr("Selected"))
                        ));
                        header_row.append(&sel_lbl);
                    } else {
                        let use_btn = gtk4::Button::with_label(&tr("Use This Font"));
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

                    let ascii_lbl = gtk4::Label::new(Some(&ascii_txt));
                    ascii_lbl.set_halign(gtk4::Align::Start);
                    ascii_lbl.set_ellipsize(pango::EllipsizeMode::End);
                    ascii_lbl.set_attributes(Some(&attrs));

                    let cjk_lbl = gtk4::Label::new(Some(&cjk_txt));
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

        {
            let r = Rc::clone(&rebuild_catalog_rows);
            btn_all.connect_toggled(move |b| {
                if b.is_active() {
                    r();
                }
            });
        }
        {
            let r = Rc::clone(&rebuild_catalog_rows);
            btn_mono.connect_toggled(move |b| {
                if b.is_active() {
                    r();
                }
            });
        }
        {
            let r = Rc::clone(&rebuild_catalog_rows);
            btn_cjk.connect_toggled(move |b| {
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

    fn build_shortcuts_tab(self: &Rc<Self>) -> gtk4::Widget {
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
                        *p.recording.borrow_mut() = Some(RecordingTarget {
                            slot_key: key_owned.clone(),
                            append: false,
                        });
                        p.recording_status_label
                            .set_text(&tr("Press shortcut keys... (Esc to cancel)"));
                        p.recording_status_label.set_visible(true);
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
                        *p.recording.borrow_mut() = Some(RecordingTarget {
                            slot_key: key_owned.clone(),
                            append: true,
                        });
                        p.recording_status_label
                            .set_text(&tr("Press shortcut keys... (Esc to cancel)"));
                        p.recording_status_label.set_visible(true);
                    }
                });
            }
            row.append(&add_btn);

            let def_btn = gtk4::Button::with_label(&tr("Default"));
            {
                let p_weak = Rc::downgrade(self);
                let key_owned = slot_key.to_string();
                let def_owned = default_val.to_string();
                def_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = p.workspace.settings();
                        apply_shortcut_slot_update(
                            &mut st.shortcuts,
                            &key_owned,
                            &def_owned,
                            false,
                        );
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
                        apply_shortcut_slot_update(&mut st.shortcuts, &key_owned, &txt, false);
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

    fn build_language_tab(self: &Rc<Self>) -> gtk4::Widget {
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        let s = self.workspace.settings();

        let lang_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let lang_lbl = gtk4::Label::new(Some(&tr("Display Language")));
        lang_lbl.set_width_chars(24);
        lang_lbl.set_halign(gtk4::Align::Start);

        #[allow(deprecated)]
        let lang_combo = gtk4::ComboBoxText::new();
        #[allow(deprecated)]
        {
            lang_combo.append(Some("auto"), &tr("Auto (System Locale)"));
            lang_combo.append(Some("en"), &tr("English (en.po)"));
            lang_combo.append(Some("ja"), &tr("日本語 (ja.po)"));
            for (code, path) in discover_po_files() {
                if code != "en" && code != "ja" {
                    lang_combo.append(Some(&code), &format!("{} ({})", code, path.display()));
                }
            }
            let _ = lang_combo.set_active_id(Some(&s.language));
        }
        lang_combo.set_hexpand(true);
        disable_combo_scroll(&lang_combo);
        lang_row.append(&lang_lbl);
        lang_row.append(&lang_combo);
        vbox.append(&lang_row);

        let custom_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let custom_lbl = gtk4::Label::new(Some(&tr("Custom .po File Path")));
        custom_lbl.set_width_chars(24);
        custom_lbl.set_halign(gtk4::Align::Start);
        let custom_entry = gtk4::Entry::new();
        custom_entry.set_text(&s.custom_po_path);
        custom_entry.set_hexpand(true);
        let reload_btn = gtk4::Button::with_label(&tr("Reload .po Files"));
        custom_row.append(&custom_lbl);
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
                #[allow(deprecated)]
                if let Some(id) = c.active_id() {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = p.workspace.settings();
                        st.language = id.to_string();
                        load_catalog_for_language(&st.language, &st.custom_po_path);
                        p.notify_changed(st);
                        p.window.set_title(Some(&tr("Termin Preferences")));
                        p.rebuild_tabs();
                    }
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

    fn refresh_presets_ui(self: &Rc<Self>) {
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

        // Top header + breadcrumb showing parent -> child path
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
                    let p_title = pr
                        .root
                        .pane_configs()
                        .get(p_idx)
                        .map(|c| c.title.clone())
                        .unwrap_or_else(|| "Shell".to_string());
                    format!(
                        "🪟 {}  →  <b>🖥 {} #{} ({})</b>",
                        tr("Window"),
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

        // 3-Column Master -> Hierarchy (Window -> Panel) -> Settings Inspector
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
                            command: None,
                            working_dir: None,
                            opacity: None,
                            font_size: None,
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
            // Root node: Window
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

            // Child nodes under Window
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

        // Quick split/add/remove toolbar at bottom of Hierarchy column
        if let Some(sel_idx) = selected_idx_opt
            && let Some(ref sel_pr) = selected_preset_opt
        {
            col2.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

            let active_pane_idx = match target {
                PresetEditorTarget::Pane(i) => i,
                PresetEditorTarget::Window => 0,
            };
            let total_panes = sel_pr.root.pane_count();

            let tree_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            let sh_btn = gtk4::Button::with_label("↔ Split H");
            sh_btn.set_tooltip_text(Some(&tr("Split Horizontally")));
            sh_btn.set_hexpand(true);
            {
                let p_weak = Rc::downgrade(self);
                sh_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(sel_idx) {
                            pr.root.split_pane_at(active_pane_idx, true);
                            let _ = st.save();
                            p.selected_preset_target
                                .set(PresetEditorTarget::Pane(active_pane_idx + 1));
                            p.refresh_presets_ui();
                        }
                    }
                });
            }

            let sv_btn = gtk4::Button::with_label("↕ Split V");
            sv_btn.set_tooltip_text(Some(&tr("Split Vertically")));
            sv_btn.set_hexpand(true);
            {
                let p_weak = Rc::downgrade(self);
                sv_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(sel_idx) {
                            pr.root.split_pane_at(active_pane_idx, false);
                            let _ = st.save();
                            p.selected_preset_target
                                .set(PresetEditorTarget::Pane(active_pane_idx + 1));
                            p.refresh_presets_ui();
                        }
                    }
                });
            }

            let tab_btn = gtk4::Button::with_label("➕ Tab");
            tab_btn.set_tooltip_text(Some(&tr("Add Tab")));
            {
                let p_weak = Rc::downgrade(self);
                tab_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(sel_idx) {
                            pr.root.add_tab_at(active_pane_idx);
                            let _ = st.save();
                            p.selected_preset_target
                                .set(PresetEditorTarget::Pane(active_pane_idx + 1));
                            p.refresh_presets_ui();
                        }
                    }
                });
            }

            let rm_btn = gtk4::Button::with_label("✖");
            rm_btn.set_tooltip_text(Some(&tr("Remove Pane")));
            rm_btn.set_sensitive(total_panes > 1);
            {
                let p_weak = Rc::downgrade(self);
                rm_btn.connect_clicked(move |_| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(sel_idx) {
                            pr.root.remove_pane_at(active_pane_idx);
                            let _ = st.save();
                            let next_idx = active_pane_idx.saturating_sub(1);
                            p.selected_preset_target
                                .set(PresetEditorTarget::Pane(next_idx));
                            p.refresh_presets_ui();
                        }
                    }
                });
            }

            tree_actions.append(&sh_btn);
            tree_actions.append(&sv_btn);
            tree_actions.append(&tab_btn);
            tree_actions.append(&rm_btn);
            col2.append(&tree_actions);
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

                // Preset Name row
                let rename_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let rename_lbl = gtk4::Label::new(Some(&tr("Preset Name")));
                rename_lbl.set_width_chars(22);
                rename_lbl.set_halign(gtk4::Align::Start);
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
                                let mut st = PresetStore::load();
                                if let Some(pr) = st.presets.get_mut(selected_idx) {
                                    pr.name = new_name;
                                    let _ = st.save();
                                    p.refresh_presets_ui();
                                }
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
                rename_row.append(&rename_lbl);
                rename_row.append(&rename_entry);
                rename_row.append(&rename_btn);
                col3.append(&rename_row);

                // Run on Startup + Quick Actions row
                let startup_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let startup_chk = gtk4::CheckButton::with_label(&tr("Run on Startup"));
                startup_chk.set_active(selected_preset.run_on_startup);
                startup_chk.set_hexpand(true);
                {
                    let p_weak = Rc::downgrade(self);
                    startup_chk.connect_toggled(move |chk| {
                        if let Some(p) = p_weak.upgrade() {
                            let mut st = PresetStore::load();
                            let target_name = st.presets.get(selected_idx).map(|pr| pr.name.clone());
                            if chk.is_active() {
                                st.set_startup_preset(target_name.as_deref());
                            } else {
                                st.set_startup_preset(None);
                            }
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
                            let mut st = PresetStore::load();
                            if let Some(existing) = st.presets.get(selected_idx).cloned() {
                                let mut captured =
                                    p.workspace.capture_current_preset(&existing.name);
                                captured.run_on_startup = existing.run_on_startup;
                                st.presets[selected_idx] = captured;
                                let _ = st.save();
                                p.refresh_presets_ui();
                            }
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

                // Window Opacity
                let win_op_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let win_op_chk = gtk4::CheckButton::with_label(&tr("Window Opacity"));
                win_op_chk.set_width_request(210);
                win_op_chk.set_active(selected_preset.window_opacity.is_some());
                let win_op_val = selected_preset.window_opacity.unwrap_or(0.95) as f64;
                let win_op_scale =
                    gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.02);
                win_op_scale.set_draw_value(true);
                win_op_scale.set_digits(2);
                win_op_scale.set_value(win_op_val);
                win_op_scale.set_hexpand(true);
                win_op_scale.set_sensitive(selected_preset.window_opacity.is_some());
                {
                    let sc = win_op_scale.clone();
                    win_op_chk.connect_toggled(move |chk| {
                        let active = chk.is_active();
                        sc.set_sensitive(active);
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            pr.window_opacity = if active {
                                Some(sc.value() as f32)
                            } else {
                                None
                            };
                            let _ = st.save();
                        }
                    });
                }
                {
                    let chk = win_op_chk.clone();
                    win_op_scale.connect_value_changed(move |sc| {
                        if chk.is_active() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.window_opacity = Some(sc.value() as f32);
                                let _ = st.save();
                            }
                        }
                    });
                }
                win_op_row.append(&win_op_chk);
                win_op_row.append(&win_op_scale);
                col3.append(&win_op_row);

                // Window Terminal Font Size (pt)
                let win_fs_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let win_fs_chk =
                    gtk4::CheckButton::with_label(&tr("Window Terminal Font Size (pt)"));
                win_fs_chk.set_width_request(210);
                win_fs_chk.set_active(selected_preset.window_font_size.is_some());
                let win_fs_val = selected_preset.window_font_size.unwrap_or(15.0) as f64;
                let win_fs_scale =
                    gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 8.0, 36.0, 0.5);
                win_fs_scale.set_draw_value(true);
                win_fs_scale.set_digits(1);
                win_fs_scale.set_value(win_fs_val);
                win_fs_scale.set_hexpand(true);
                win_fs_scale.set_sensitive(selected_preset.window_font_size.is_some());
                {
                    let sc = win_fs_scale.clone();
                    win_fs_chk.connect_toggled(move |chk| {
                        let active = chk.is_active();
                        sc.set_sensitive(active);
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            pr.window_font_size = if active {
                                Some(((sc.value() as f32) * 2.0).round() / 2.0)
                            } else {
                                None
                            };
                            let _ = st.save();
                        }
                    });
                }
                {
                    let chk = win_fs_chk.clone();
                    win_fs_scale.connect_value_changed(move |sc| {
                        if chk.is_active() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.window_font_size =
                                    Some(((sc.value() as f32) * 2.0).round() / 2.0);
                                let _ = st.save();
                            }
                        }
                    });
                }
                win_fs_row.append(&win_fs_chk);
                win_fs_row.append(&win_fs_scale);
                col3.append(&win_fs_row);

                // Window UI Font Size (pt)
                let win_uifs_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let win_uifs_chk = gtk4::CheckButton::with_label(&tr("Window UI Font Size (pt)"));
                win_uifs_chk.set_width_request(210);
                win_uifs_chk.set_active(selected_preset.ui_font_size.is_some());
                let win_uifs_val = selected_preset.ui_font_size.unwrap_or(15.0) as f64;
                let win_uifs_scale =
                    gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 9.0, 28.0, 0.5);
                win_uifs_scale.set_draw_value(true);
                win_uifs_scale.set_digits(1);
                win_uifs_scale.set_value(win_uifs_val);
                win_uifs_scale.set_hexpand(true);
                win_uifs_scale.set_sensitive(selected_preset.ui_font_size.is_some());
                {
                    let sc = win_uifs_scale.clone();
                    win_uifs_chk.connect_toggled(move |chk| {
                        let active = chk.is_active();
                        sc.set_sensitive(active);
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            pr.ui_font_size = if active {
                                Some(((sc.value() as f32) * 2.0).round() / 2.0)
                            } else {
                                None
                            };
                            let _ = st.save();
                        }
                    });
                }
                {
                    let chk = win_uifs_chk.clone();
                    win_uifs_scale.connect_value_changed(move |sc| {
                        if chk.is_active() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.ui_font_size = Some(((sc.value() as f32) * 2.0).round() / 2.0);
                                let _ = st.save();
                            }
                        }
                    });
                }
                win_uifs_row.append(&win_uifs_chk);
                win_uifs_row.append(&win_uifs_scale);
                col3.append(&win_uifs_row);
            }
            PresetEditorTarget::Pane(pane_idx) => {
                let pane_cfgs = selected_preset.root.pane_configs();
                let total_panes = pane_cfgs.len();
                let cfg = pane_cfgs.get(pane_idx).cloned().unwrap_or_default();

                let hdr_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                let pane_lbl = gtk4::Label::new(None);
                pane_lbl.set_markup(&format!(
                    "<b>3. 🖥 {} #{} — {}</b>",
                    tr("Pane"),
                    pane_idx + 1,
                    tr("Panel Settings")
                ));
                pane_lbl.set_halign(gtk4::Align::Start);
                pane_lbl.set_hexpand(true);
                hdr_row.append(&pane_lbl);
                col3.append(&hdr_row);

                // Split / Tab / Remove actions for this panel
                let struct_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                let split_h_btn = gtk4::Button::with_label(&tr("Split Horizontally"));
                {
                    let p_weak = Rc::downgrade(self);
                    split_h_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.root.split_pane_at(pane_idx, true);
                                let _ = st.save();
                                p.selected_preset_target
                                    .set(PresetEditorTarget::Pane(pane_idx + 1));
                                p.refresh_presets_ui();
                            }
                        }
                    });
                }

                let split_v_btn = gtk4::Button::with_label(&tr("Split Vertically"));
                {
                    let p_weak = Rc::downgrade(self);
                    split_v_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.root.split_pane_at(pane_idx, false);
                                let _ = st.save();
                                p.selected_preset_target
                                    .set(PresetEditorTarget::Pane(pane_idx + 1));
                                p.refresh_presets_ui();
                            }
                        }
                    });
                }

                let add_tab_btn = gtk4::Button::with_label(&tr("Add Tab"));
                {
                    let p_weak = Rc::downgrade(self);
                    add_tab_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.root.add_tab_at(pane_idx);
                                let _ = st.save();
                                p.selected_preset_target
                                    .set(PresetEditorTarget::Pane(pane_idx + 1));
                                p.refresh_presets_ui();
                            }
                        }
                    });
                }

                let rm_pane_btn = gtk4::Button::with_label(&tr("Remove Pane"));
                rm_pane_btn.set_sensitive(total_panes > 1);
                {
                    let p_weak = Rc::downgrade(self);
                    rm_pane_btn.connect_clicked(move |_| {
                        if let Some(p) = p_weak.upgrade() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.root.remove_pane_at(pane_idx);
                                let _ = st.save();
                                let next_idx = pane_idx.saturating_sub(1);
                                p.selected_preset_target
                                    .set(PresetEditorTarget::Pane(next_idx));
                                p.refresh_presets_ui();
                            }
                        }
                    });
                }

                struct_row.append(&split_h_btn);
                struct_row.append(&split_v_btn);
                struct_row.append(&add_tab_btn);
                struct_row.append(&rm_pane_btn);
                col3.append(&struct_row);

                col3.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

                // Title row
                let title_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let title_lbl = gtk4::Label::new(Some(&tr("Pane Title")));
                title_lbl.set_width_chars(22);
                title_lbl.set_halign(gtk4::Align::Start);
                let title_entry = gtk4::Entry::new();
                title_entry.set_text(&cfg.title);
                title_entry.set_hexpand(true);
                {
                    let labels_map = Rc::clone(&pane_tree_labels);
                    title_entry.connect_changed(move |e| {
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            let txt = e.text().to_string();
                            let effective_title = if txt.trim().is_empty() {
                                "Shell".to_string()
                            } else {
                                txt
                            };
                            pr.root.update_pane_at(pane_idx, |c| {
                                c.title = effective_title.clone();
                            });
                            let _ = st.save();
                            if let Some(tree_lbl) = labels_map.borrow().get(&pane_idx) {
                                tree_lbl.set_markup(&format!(
                                    "🖥 <b>{} #{}</b> <small>({})</small>",
                                    tr("Pane"),
                                    pane_idx + 1,
                                    glib::markup_escape_text(&effective_title)
                                ));
                            }
                        }
                    });
                }
                title_row.append(&title_lbl);
                title_row.append(&title_entry);
                col3.append(&title_row);

                // Working dir row
                let wd_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let wd_lbl = gtk4::Label::new(Some(&tr("Working Directory")));
                wd_lbl.set_width_chars(22);
                wd_lbl.set_halign(gtk4::Align::Start);
                let wd_entry = gtk4::Entry::new();
                wd_entry.set_placeholder_text(Some("~/projects"));
                wd_entry.set_text(cfg.working_dir.as_deref().unwrap_or(""));
                wd_entry.set_hexpand(true);
                {
                    wd_entry.connect_changed(move |e| {
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            let txt = e.text().trim().to_string();
                            pr.root.update_pane_at(pane_idx, |c| {
                                c.working_dir = if txt.is_empty() {
                                    None
                                } else {
                                    Some(txt.clone())
                                };
                            });
                            let _ = st.save();
                        }
                    });
                }
                wd_row.append(&wd_lbl);
                wd_row.append(&wd_entry);
                col3.append(&wd_row);

                // Command row
                let cmd_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let cmd_lbl = gtk4::Label::new(Some(&tr("Auto-Run Command")));
                cmd_lbl.set_width_chars(22);
                cmd_lbl.set_halign(gtk4::Align::Start);
                let cmd_entry = gtk4::Entry::new();
                cmd_entry.set_placeholder_text(Some("e.g. nvim, htop, cargo test"));
                cmd_entry.set_text(cfg.command.as_deref().unwrap_or(""));
                cmd_entry.set_hexpand(true);
                {
                    cmd_entry.connect_changed(move |e| {
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            let txt = e.text().trim().to_string();
                            pr.root.update_pane_at(pane_idx, |c| {
                                c.command =
                                    if txt.is_empty() { None } else { Some(txt.clone()) };
                            });
                            let _ = st.save();
                        }
                    });
                }
                cmd_row.append(&cmd_lbl);
                cmd_row.append(&cmd_entry);
                col3.append(&cmd_row);

                // Panel Opacity row
                let p_op_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let p_op_chk = gtk4::CheckButton::with_label(&tr("Panel Opacity"));
                p_op_chk.set_width_request(198);
                p_op_chk.set_active(cfg.opacity.is_some());
                let p_op_val = cfg.opacity.unwrap_or(0.95) as f64;
                let p_op_scale =
                    gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.02);
                p_op_scale.set_draw_value(true);
                p_op_scale.set_digits(2);
                p_op_scale.set_value(p_op_val);
                p_op_scale.set_hexpand(true);
                p_op_scale.set_sensitive(cfg.opacity.is_some());
                {
                    let sc = p_op_scale.clone();
                    p_op_chk.connect_toggled(move |chk| {
                        let active = chk.is_active();
                        sc.set_sensitive(active);
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            pr.root.update_pane_at(pane_idx, |c| {
                                c.opacity = if active {
                                    Some(sc.value() as f32)
                                } else {
                                    None
                                };
                            });
                            let _ = st.save();
                        }
                    });
                }
                {
                    let chk = p_op_chk.clone();
                    p_op_scale.connect_value_changed(move |sc| {
                        if chk.is_active() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.root.update_pane_at(pane_idx, |c| {
                                    c.opacity = Some(sc.value() as f32);
                                });
                                let _ = st.save();
                            }
                        }
                    });
                }
                p_op_row.append(&p_op_chk);
                p_op_row.append(&p_op_scale);
                col3.append(&p_op_row);

                // Panel Font Size row
                let p_fs_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let p_fs_chk = gtk4::CheckButton::with_label(&tr("Panel Font Size (pt)"));
                p_fs_chk.set_width_request(198);
                p_fs_chk.set_active(cfg.font_size.is_some());
                let p_fs_val = cfg.font_size.unwrap_or(15.0) as f64;
                let p_fs_scale =
                    gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 8.0, 36.0, 0.5);
                p_fs_scale.set_draw_value(true);
                p_fs_scale.set_digits(1);
                p_fs_scale.set_value(p_fs_val);
                p_fs_scale.set_hexpand(true);
                p_fs_scale.set_sensitive(cfg.font_size.is_some());
                {
                    let sc = p_fs_scale.clone();
                    p_fs_chk.connect_toggled(move |chk| {
                        let active = chk.is_active();
                        sc.set_sensitive(active);
                        let mut st = PresetStore::load();
                        if let Some(pr) = st.presets.get_mut(selected_idx) {
                            pr.root.update_pane_at(pane_idx, |c| {
                                c.font_size = if active {
                                    Some(((sc.value() as f32) * 2.0).round() / 2.0)
                                } else {
                                    None
                                };
                            });
                            let _ = st.save();
                        }
                    });
                }
                {
                    let chk = p_fs_chk.clone();
                    p_fs_scale.connect_value_changed(move |sc| {
                        if chk.is_active() {
                            let mut st = PresetStore::load();
                            if let Some(pr) = st.presets.get_mut(selected_idx) {
                                pr.root.update_pane_at(pane_idx, |c| {
                                    c.font_size =
                                        Some(((sc.value() as f32) * 2.0).round() / 2.0);
                                });
                                let _ = st.save();
                            }
                        }
                    });
                }
                p_fs_row.append(&p_fs_chk);
                p_fs_row.append(&p_fs_scale);
                col3.append(&p_fs_row);
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
            LayoutNode::Pane { title, command, .. } => {
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
                    "🖥 <b>{} #{}</b> <small>({}{})</small>",
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

/// ComboBox 上でタッチパッドスワイプやマウスホイールを回した際に選択項目が勝手に変わるのを防ぎます。
fn disable_combo_scroll(widget: &impl IsA<gtk4::Widget>) {
    let scroll_ctrl = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
    scroll_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    scroll_ctrl.connect_scroll(|_, _, _| glib::Propagation::Stop);
    widget.add_controller(scroll_ctrl);
}

fn color_status_text(raw: &str) -> String {
    if raw.trim().is_empty() {
        tr("Using Default")
    } else if parse_color(raw).is_some() {
        tr("Applied")
    } else {
        tr("Invalid Color (Fallback to Default)")
    }
}

fn get_shortcut_slot_value(sc: &ShortcutSettings, slot_key: &str) -> String {
    sc.get_slot(slot_key)
}

fn apply_shortcut_slot_update(
    sc: &mut ShortcutSettings,
    slot_key: &str,
    new_val: &str,
    append: bool,
) {
    sc.update_slot(slot_key, new_val, append);
}