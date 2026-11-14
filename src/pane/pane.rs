// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::Widget;

use super::interaction::PaneInteraction;
use super::types::PaneEvent;
#[cfg(test)]
use super::types::{PaneKind, PaneShortcutAction, SpecialModeKey};
use crate::config::theme::ThemeSettings;
use crate::ui::state::{PaneContext, TileId};
use crate::ui::titlebar::TitlebarWidget;
use crate::vte::{create_terminal_backend, TerminalBackend};

/// Computes the clamped zoom multiplier according to Section 10.5.3:
/// `zoom_scale` in `0.4..=3.5` and effective font size in `6.0..=64.0` px.
pub fn compute_clamped_zoom(target_scale: f64, base_font_size: f32) -> f64 {
    let base = (base_font_size as f64).max(6.0);
    let min_by_px = (6.0 / base).max(0.4);
    let max_by_px = (64.0 / base).min(3.5);
    target_scale.clamp(min_by_px, max_by_px.max(min_by_px))
}

/// Pure keyboard-to-VT byte encoder matching Section 10.5.4.
/// Used by unit tests and custom non-VTE backends when `vte4` is swapped out.
pub fn encode_terminal_key_bytes(
    key_name: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
) -> Option<Vec<u8>> {
    if ctrl && !alt {
        if key_name.len() == 1 {
            let ch = key_name.chars().next().unwrap();
            if ch.is_ascii_alphabetic() {
                let upper = ch.to_ascii_uppercase() as u8;
                return Some(vec![upper & 0x1F]);
            }
            match ch {
                ' ' => return Some(vec![0x00]),
                '/' => return Some(vec![0x1F]),
                '[' => return Some(vec![0x1B]),
                '\\' => return Some(vec![0x1C]),
                ']' => return Some(vec![0x1D]),
                _ => {}
            }
        }
        if key_name.eq_ignore_ascii_case("space") {
            return Some(vec![0x00]);
        }
    }

    if alt && !ctrl && key_name.len() == 1 {
        let ch = key_name.chars().next().unwrap();
        if ch.is_ascii_alphanumeric() {
            return Some(vec![0x1B, ch.to_ascii_lowercase() as u8]);
        }
    }

    if shift && key_name.eq_ignore_ascii_case("tab") {
        return Some(b"\x1b[Z".to_vec());
    }

    match key_name.to_ascii_lowercase().as_str() {
        "enter" | "return" => Some(b"\r".to_vec()),
        "backspace" => Some(vec![0x7F]),
        "tab" => Some(b"\t".to_vec()),
        "escape" => Some(vec![0x1B]),
        "up" => Some(b"\x1b[A".to_vec()),
        "down" => Some(b"\x1b[B".to_vec()),
        "right" => Some(b"\x1b[C".to_vec()),
        "left" => Some(b"\x1b[D".to_vec()),
        "home" => Some(b"\x1b[H".to_vec()),
        "end" => Some(b"\x1b[F".to_vec()),
        "insert" => Some(b"\x1b[2~".to_vec()),
        "delete" => Some(b"\x1b[3~".to_vec()),
        "pageup" => Some(b"\x1b[5~".to_vec()),
        "pagedown" => Some(b"\x1b[6~".to_vec()),
        "f1" => Some(b"\x1bOP".to_vec()),
        "f2" => Some(b"\x1bOQ".to_vec()),
        "f3" => Some(b"\x1bOR".to_vec()),
        "f4" => Some(b"\x1bOS".to_vec()),
        "f5" => Some(b"\x1b[15~".to_vec()),
        "f6" => Some(b"\x1b[17~".to_vec()),
        "f7" => Some(b"\x1b[18~".to_vec()),
        "f8" => Some(b"\x1b[19~".to_vec()),
        "f9" => Some(b"\x1b[20~".to_vec()),
        "f10" => Some(b"\x1b[21~".to_vec()),
        "f11" => Some(b"\x1b[23~".to_vec()),
        "f12" => Some(b"\x1b[24~".to_vec()),
        _ => {
            if !ctrl && !alt && key_name.chars().count() == 1 {
                Some(key_name.as_bytes().to_vec())
            } else {
                None
            }
        }
    }
}

pub struct TerminalPaneWidget {
    pub tile_id: TileId,
    pub root: gtk4::Box,
    pub titlebar: Rc<TitlebarWidget>,
    pub scrollbar: gtk4::Scrollbar,
    pub scroll_badge: gtk4::Label,
    pub highlight_frame: gtk4::Box,
    interaction: Rc<PaneInteraction>,
    backend: Rc<RefCell<Box<dyn TerminalBackend>>>,
    custom_opacity: Cell<Option<f32>>,
    custom_font_size: Cell<Option<f32>>,
    custom_show_scrollbar: Cell<Option<bool>>,
    read_only: Cell<bool>,
    initial_working_dir: RefCell<Option<PathBuf>>,
    startup_command_str: RefCell<Option<String>>,
    pinch_initial_zoom: Rc<Cell<f64>>,
}

impl TerminalPaneWidget {
    pub fn new<F>(
        tile_id: TileId,
        initial_title: &str,
        working_dir: Option<&Path>,
        startup_command: Option<&str>,
        settings: &ThemeSettings,
        on_event: F,
    ) -> Rc<Self>
    where
        F: Fn(PaneEvent) + 'static,
    {
        let on_event: Rc<dyn Fn(PaneEvent)> = Rc::new(on_event);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.add_css_class("fwer-pane");

        let pane_overlay = gtk4::Overlay::new();
        pane_overlay.set_hexpand(true);
        pane_overlay.set_vexpand(true);

        let inner_vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        inner_vbox.set_hexpand(true);
        inner_vbox.set_vexpand(true);

        let tb_cb = Rc::clone(&on_event);
        let titlebar = TitlebarWidget::new(tile_id, initial_title, move |tb_ev| {
            tb_cb(PaneEvent::Titlebar(tb_ev));
        });
        inner_vbox.append(&titlebar.root);

        let mut backend_box = create_terminal_backend(settings);
        let _ = backend_box.spawn_shell(working_dir, startup_command);

        let term_widget = backend_box.widget();
        let vadj = backend_box.vadjustment();
        let scrollbar = gtk4::Scrollbar::new(gtk4::Orientation::Vertical, vadj.as_ref());
        scrollbar.set_visible(settings.show_scrollbar);
        scrollbar.add_css_class("fwer-scrollbar");

        let content_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        content_row.set_hexpand(true);
        content_row.set_vexpand(true);
        content_row.append(&term_widget);
        content_row.append(&scrollbar);

        let overlay = gtk4::Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        overlay.set_child(Some(&content_row));

        let scroll_badge = gtk4::Label::new(None);
        scroll_badge.add_css_class("fwer-scroll-badge");
        scroll_badge.set_halign(gtk4::Align::End);
        scroll_badge.set_valign(gtk4::Align::Start);
        scroll_badge.set_margin_top(6);
        scroll_badge.set_margin_end(18);
        scroll_badge.set_visible(false);
        scroll_badge.set_can_target(false);
        overlay.add_overlay(&scroll_badge);

        inner_vbox.append(&overlay);
        pane_overlay.set_child(Some(&inner_vbox));

        let interaction = PaneInteraction::new(
            tile_id,
            &root,
            &pane_overlay,
            settings,
            Rc::clone(&on_event),
        );
        let highlight_frame = interaction.highlight_frame.clone();

        root.append(&pane_overlay);

        {
            let bcast_cb = Rc::clone(&on_event);
            backend_box.set_on_user_input(Box::new(move |bytes| {
                bcast_cb(PaneEvent::BroadcastInput {
                    source_id: tile_id,
                    bytes: bytes.to_vec(),
                });
            }));
        }
        {
            let exit_cb = Rc::clone(&on_event);
            backend_box.set_on_child_exited(Box::new(move |_status| {
                exit_cb(PaneEvent::ChildExited(tile_id));
            }));
        }
        {
            let badge_clone = scroll_badge.clone();
            backend_box.set_on_scroll_changed(Box::new(move |offset, max_offset| {
                if offset > 0 && max_offset > 0 {
                    badge_clone.set_text(&format!("↑ -{}/{}", offset, max_offset));
                    badge_clone.set_visible(true);
                } else {
                    badge_clone.set_visible(false);
                }
            }));
        }

        let pane_rc = Rc::new(Self {
            tile_id,
            root,
            titlebar,
            scrollbar,
            scroll_badge,
            highlight_frame,
            interaction,
            backend: Rc::new(RefCell::new(backend_box)),
            custom_opacity: Cell::new(None),
            custom_font_size: Cell::new(None),
            custom_show_scrollbar: Cell::new(None),
            read_only: Cell::new(false),
            initial_working_dir: RefCell::new(working_dir.map(|p| p.to_path_buf())),
            startup_command_str: RefCell::new(startup_command.map(|s| s.to_string())),
            pinch_initial_zoom: Rc::new(Cell::new(1.0)),
        });

        pane_rc.setup_controllers(&term_widget);
        pane_rc
    }

    fn setup_controllers(self: &Rc<Self>, term_widget: &Widget) {
        self.interaction.attach_click_controller(term_widget);

        // Super + Mouse Wheel Scroll -> Per-pane zoom & Opacity Adjust Mode scroll
        {
            let scroll_ctrl =
                gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
            scroll_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let pane_weak = Rc::downgrade(self);
            scroll_ctrl.connect_scroll(move |ctrl, _dx, dy| {
                let Some(pane) = pane_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if pane.interaction.handle_opacity_scroll(dy) == glib::Propagation::Stop {
                    return glib::Propagation::Stop;
                }
                let mods = ctrl.current_event_state();
                if mods.contains(gdk::ModifierType::SUPER_MASK)
                    || mods.contains(gdk::ModifierType::META_MASK)
                {
                    let step = if dy < 0.0 { 0.1 } else { -0.1 };
                    pane.adjust_zoom_by(step);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            term_widget.add_controller(scroll_ctrl);
        }

        // Touchpad pinch-to-zoom via GestureZoom
        {
            let gesture_zoom = gtk4::GestureZoom::new();
            let pane_weak1 = Rc::downgrade(self);
            gesture_zoom.connect_begin(move |_, _| {
                if let Some(pane) = pane_weak1.upgrade() {
                    let cur = pane.backend.borrow().zoom_scale();
                    pane.pinch_initial_zoom.set(cur);
                }
            });
            let pane_weak2 = Rc::downgrade(self);
            gesture_zoom.connect_scale_changed(move |_, scale_delta| {
                if let Some(pane) = pane_weak2.upgrade() {
                    let base_zoom = pane.pinch_initial_zoom.get();
                    pane.set_zoom_scale(base_zoom * scale_delta);
                }
            });
            term_widget.add_controller(gesture_zoom);
        }

        // Keyboard shortcuts & Shift+Navigation scrollback controls
        {
            let key_ctrl = gtk4::EventControllerKey::new();
            key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let pane_weak = Rc::downgrade(self);
            key_ctrl.connect_key_pressed(move |_ctrl, key, _keycode, mods| {
                let Some(pane) = pane_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if pane.interaction.handle_common_key_press(key, mods) {
                    return glib::Propagation::Stop;
                }

                let only_shift = mods.contains(gdk::ModifierType::SHIFT_MASK)
                    && !mods.contains(gdk::ModifierType::CONTROL_MASK)
                    && !mods.contains(gdk::ModifierType::ALT_MASK)
                    && !mods.contains(gdk::ModifierType::SUPER_MASK);

                if only_shift {
                    match key {
                        gdk::Key::Page_Up | gdk::Key::KP_Page_Up => {
                            pane.backend.borrow_mut().scroll_page(true);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Page_Down | gdk::Key::KP_Page_Down => {
                            pane.backend.borrow_mut().scroll_page(false);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Up | gdk::Key::KP_Up => {
                            pane.backend.borrow_mut().scroll_lines(-1);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Down | gdk::Key::KP_Down => {
                            pane.backend.borrow_mut().scroll_lines(1);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Home | gdk::Key::KP_Home => {
                            pane.backend.borrow_mut().scroll_to_top();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::End | gdk::Key::KP_End => {
                            pane.backend.borrow_mut().scroll_to_bottom();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Insert | gdk::Key::KP_Insert => {
                            pane.paste_clipboard();
                            return glib::Propagation::Stop;
                        }
                        _ => {}
                    }
                }

                if pane.is_read_only() {
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            });
            term_widget.add_controller(key_ctrl);
        }
    }

    pub fn set_keyboard_target(&self, targeted: bool) {
        self.interaction.set_keyboard_target(targeted);
    }

    pub fn set_opacity_highlight(&self, is_source: bool, confirmed: bool) {
        self.interaction.set_opacity_highlight(is_source, confirmed);
    }

    pub fn custom_opacity(&self) -> Option<f32> {
        self.custom_opacity.get()
    }

    pub fn effective_opacity(&self, settings: &ThemeSettings) -> f32 {
        self.custom_opacity
            .get()
            .unwrap_or_else(|| settings.effective_terminal_bg_opacity())
            .clamp(0.0, 1.0)
    }

    pub fn set_custom_opacity(&self, opacity: Option<f32>) {
        self.custom_opacity.set(opacity.map(|v| v.clamp(0.0, 1.0)));
        let settings = self.interaction.settings();
        self.apply_effective_theme(&settings);
    }

    pub fn adjust_custom_opacity_by(&self, delta: f32, settings: &ThemeSettings) {
        let base = self.effective_opacity(settings);
        let next = (base + delta).clamp(0.0, 1.0);
        self.set_custom_opacity(Some(next));
    }

    pub fn custom_font_size(&self) -> Option<f32> {
        self.custom_font_size.get()
    }

    pub fn effective_font_size(&self, settings: &ThemeSettings) -> f32 {
        self.custom_font_size
            .get()
            .unwrap_or_else(|| settings.effective_terminal_font_size())
            .clamp(8.0, 36.0)
    }

    pub fn set_custom_font_size(&self, font_size: Option<f32>) {
        self.custom_font_size
            .set(font_size.map(|v| ((v.clamp(8.0, 36.0) * 2.0).round()) / 2.0));
        let settings = self.interaction.settings();
        self.apply_effective_theme(&settings);
    }

    pub fn custom_show_scrollbar(&self) -> Option<bool> {
        self.custom_show_scrollbar.get()
    }

    pub fn effective_show_scrollbar(&self, settings: &ThemeSettings) -> bool {
        self.custom_show_scrollbar
            .get()
            .unwrap_or(settings.show_scrollbar)
    }

    pub fn set_custom_show_scrollbar(&self, show: Option<bool>) {
        self.custom_show_scrollbar.set(show);
        let settings = self.interaction.settings();
        self.scrollbar
            .set_visible(self.effective_show_scrollbar(&settings));
    }

    pub fn toggle_scrollbar(&self) {
        let cur = self.effective_show_scrollbar(&self.interaction.settings());
        self.set_custom_show_scrollbar(Some(!cur));
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only.get()
    }

    pub fn set_read_only(&self, read_only: bool) {
        self.read_only.set(read_only);
        self.backend.borrow_mut().set_read_only(read_only);
        self.titlebar.set_read_only(read_only);
    }

    pub fn toggle_read_only(&self) {
        let cur = self.is_read_only();
        self.set_read_only(!cur);
    }

    pub fn startup_command(&self) -> Option<String> {
        self.startup_command_str.borrow().clone()
    }

    pub fn set_startup_command(&self, cmd: Option<String>) {
        *self.startup_command_str.borrow_mut() = cmd;
    }

    fn apply_effective_theme(&self, settings: &ThemeSettings) {
        let mut eff = settings.clone();
        eff.terminal_bg_opacity = self.effective_opacity(settings);
        eff.terminal_font_size = self.effective_font_size(settings);
        self.backend.borrow_mut().apply_theme(&eff);
    }

    pub fn refresh_highlight_frame(&self) {
        self.interaction.refresh_highlight_frame();
    }

    pub fn grab_terminal_focus(&self) {
        self.backend.borrow().grab_focus();
    }

    pub fn finish_title_edit_if_active(&self) -> Option<String> {
        self.titlebar.finish_editing_if_active()
    }

    pub fn write_input(&self, data: &[u8]) {
        if self.read_only.get() {
            return;
        }
        self.backend.borrow_mut().write_input(data);
    }

    pub fn copy_selection_or_screen(&self) {
        self.backend.borrow().copy_selection_or_screen();
    }

    pub fn paste_clipboard(&self) {
        if self.read_only.get() {
            return;
        }
        self.backend.borrow().paste_clipboard();
    }

    pub fn clear_scrollback(&self) {
        self.backend.borrow_mut().clear_scrollback();
    }

    pub fn has_selection(&self) -> bool {
        self.backend.borrow().has_selection()
    }

    pub fn is_alive(&self) -> bool {
        self.backend.borrow().is_alive()
    }

    pub fn detach_callbacks(&self) {
        self.backend.borrow_mut().clear_callbacks();
    }

    pub fn current_working_dir(&self) -> Option<PathBuf> {
        self.backend
            .borrow()
            .current_working_dir()
            .or_else(|| self.initial_working_dir.borrow().clone())
    }

    pub fn set_zoom_scale(&self, scale: f64) {
        let base_font = self.effective_font_size(&self.interaction.settings());
        let clamped = compute_clamped_zoom(scale, base_font);
        self.backend.borrow_mut().set_zoom_scale(clamped);
    }

    pub fn adjust_zoom_by(&self, delta: f64) {
        let cur = self.backend.borrow().zoom_scale();
        self.set_zoom_scale(cur + delta);
    }

    pub fn reset_zoom(&self) {
        self.set_zoom_scale(1.0);
    }

    pub fn update_context_and_theme(&self, ctx: &PaneContext, settings: &ThemeSettings) {
        self.scrollbar
            .set_visible(self.effective_show_scrollbar(settings));
        self.titlebar.set_read_only(self.read_only.get());
        self.titlebar.update_appearance(ctx, settings);
        self.interaction.update_context_and_settings(ctx, settings);
        self.apply_effective_theme(settings);
    }
}

#[cfg(test)]
#[path = "test/pane_tests.rs"]
mod pane_tests;
