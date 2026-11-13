// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

#![allow(deprecated)]
pub mod font_scanner;
pub mod layout_preset;
pub mod shortcuts;
pub mod theme;

mod font_catalog;
mod tab_appearance;
mod tab_font;
mod tab_language;
mod tab_presets;
mod tab_shortcuts;
pub mod widgets;

// 既存の `use crate::config::prefs_ui::PreferencesWindow;` をそのまま通すための互換モジュール
pub mod prefs_ui {
    pub use super::PreferencesWindow;
}

pub use layout_preset::{
    expand_tilde_path, layout_file_path, presets_file_path, LayoutNode, PanePresetConfig,
    PresetStore, WorkspacePreset,
};
pub use theme::*;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::{
    gdk, glib, prelude::*, Box as GtkBox, Button, Entry, EventControllerKey, Label, Orientation,
    PropagationPhase, Revealer, RevealerTransitionType, ScrolledWindow, Separator, Stack,
    StackTransitionType, Viewport, Widget, Window,
};

use crate::i18n::{load_catalog_for_language, tr};
use crate::ui::workspace::Workspace;

#[derive(Debug, Clone)]
pub(super) struct RecordingTarget {
    pub slot_key: String,
    pub append: bool,
}

#[derive(Clone)]
pub(super) struct ShortcutRowWidgets {
    pub entry: Entry,
    pub chips_box: GtkBox,
    pub updating: Rc<Cell<bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PresetEditorTarget {
    Window,
    Pane(usize),
}

pub struct PreferencesWindow {
    pub window: Window,
    pub(super) workspace: Rc<Workspace>,
    pub(super) stack: Stack,
    pub(super) body_revealer: Revealer,
    pub(super) collapsed: Cell<bool>,
    pub(super) save_pending: Cell<bool>,
    pub(super) recording: Rc<RefCell<Option<RecordingTarget>>>,
    pub(super) recording_status_label: Label,
    pub(super) shortcut_rows: RefCell<HashMap<String, ShortcutRowWidgets>>,
    pub(super) discovered_fonts: Rc<RefCell<Vec<PathBuf>>>,
    pub(super) fonts_loaded: Cell<bool>,
    pub(super) font_tab_populated: Cell<bool>,
    pub(super) selected_preset_index: Cell<Option<usize>>,
    pub(super) selected_preset_target: Cell<PresetEditorTarget>,
    pub(super) presets_box: GtkBox,
    pub(super) catalog_window: RefCell<Option<Window>>,
    pub(super) on_settings_applied: Rc<dyn Fn(&ThemeSettings)>,
}

impl PreferencesWindow {
    pub fn new(
        parent: &impl IsA<Window>,
        workspace: Rc<Workspace>,
        on_settings_applied: impl Fn(&ThemeSettings) + 'static,
    ) -> Rc<Self> {
        let window = Window::builder()
            .title(tr("Preferences"))
            .transient_for(parent)
            .default_width(920)
            .default_height(560)
            .width_request(720)
            .height_request(420)
            .resizable(true)
            .build();

        window.add_css_class("fwer-prefs-window");
        let stack = Stack::new();
        stack.set_transition_type(StackTransitionType::Crossfade);
        stack.set_hexpand(true);
        stack.set_vexpand(true);

        let body_revealer = Revealer::new();
        body_revealer.set_transition_type(RevealerTransitionType::SlideDown);
        body_revealer.set_reveal_child(true);
        body_revealer.set_hexpand(true);
        body_revealer.set_vexpand(true);

        let recording = Rc::new(RefCell::new(None));
        let recording_status_label = Label::new(None);
        recording_status_label.add_css_class("fwer-record-banner");
        recording_status_label.set_visible(false);

        let discovered_fonts = Rc::new(RefCell::new(Vec::new()));
        let presets_box = GtkBox::new(Orientation::Vertical, 10);
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

    pub(super) fn ensure_fonts_loaded(&self) {
        if !self.fonts_loaded.get() {
            *self.discovered_fonts.borrow_mut() = cached_system_fonts(false);
            self.fonts_loaded.set(true);
        }
    }

    pub fn present(self: &Rc<Self>) {
        self.window.set_title(Some(&tr("Preferences")));
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

    pub(super) fn notify_changed(self: &Rc<Self>, settings: ThemeSettings) {
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
        let outer = GtkBox::new(Orientation::Vertical, 0);
        outer.set_hexpand(true);
        outer.set_vexpand(true);

        let header_bar = GtkBox::new(Orientation::Horizontal, 6);
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
            let btn = Button::with_label(&title);
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

        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header_bar.append(&spacer);

        let collapse_btn = Button::with_label("▾");
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

        let sep1 = Separator::new(Orientation::Horizontal);
        outer.append(&header_bar);
        outer.append(&sep1);
        outer.append(&self.recording_status_label);

        let body_box = GtkBox::new(Orientation::Vertical, 0);
        body_box.set_hexpand(true);
        body_box.set_vexpand(true);

        let sep2 = Separator::new(Orientation::Horizontal);
        body_box.append(&self.stack);
        body_box.append(&sep2);

        let footer = GtkBox::new(Orientation::Horizontal, 8);
        footer.set_margin_top(8);
        footer.set_margin_bottom(8);
        footer.set_margin_start(12);
        footer.set_margin_end(12);

        let restore_btn = Button::with_label(&tr("Restore Defaults"));
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

        let save_layout_btn = Button::with_label(&tr("Save Current Layout"));
        {
            let p_weak = Rc::downgrade(self);
            save_layout_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let _ = p.workspace.save_current_layout();
                }
            });
        }

        let load_layout_btn = Button::with_label(&tr("Load Saved Layout"));
        {
            let p_weak = Rc::downgrade(self);
            load_layout_btn.connect_clicked(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    let _ = p.workspace.load_saved_layout();
                }
            });
        }

        let footer_spacer = GtkBox::new(Orientation::Horizontal, 0);
        footer_spacer.set_hexpand(true);

        let close_btn = Button::with_label(&tr("Close"));
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

        let key_ctrl = EventControllerKey::new();
        key_ctrl.set_propagation_phase(PropagationPhase::Capture);
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
                    settings
                        .shortcuts
                        .update_slot(&target.slot_key, &formatted, target.append);
                    p.notify_changed(settings);
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

    pub(super) fn wrap_tab_scrolled(child: &impl IsA<Widget>) -> ScrolledWindow {
        ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .child(child)
            .build()
    }

    pub(super) fn rebuild_tabs(self: &Rc<Self>) {
        let cur_tab = self
            .stack
            .visible_child_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "titlebar".to_string());

        if let Some(vp) = self
            .presets_box
            .parent()
            .and_then(|p| p.downcast::<Viewport>().ok())
        {
            vp.set_child(Widget::NONE);
        }

        while let Some(child) = self.stack.first_child() {
            self.stack.remove(&child);
        }

        let tab1 = Self::wrap_tab_scrolled(&self.build_titlebar_and_frame_tab());
        let tab2: Widget = if cur_tab == "font" {
            self.font_tab_populated.set(true);
            Self::wrap_tab_scrolled(&self.build_font_tab()).upcast()
        } else {
            self.font_tab_populated.set(false);
            GtkBox::new(Orientation::Vertical, 0).upcast()
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

    pub(super) fn populate_font_tab_now(self: &Rc<Self>) {
        if let Some(old_font_tab) = self.stack.child_by_name("font") {
            self.stack.remove(&old_font_tab);
        }
        let tab2 = Self::wrap_tab_scrolled(&self.build_font_tab());
        self.stack.add_named(&tab2, Some("font"));
        self.font_tab_populated.set(true);
    }
}
