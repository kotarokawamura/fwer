// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::config::theme::{ThemeSettings, broadcast_group_title_bg, format_rgb_hex};
use crate::ui::state::{InteractionMode, PaneContext, TileId};
use crate::vte::RgbColor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitlebarEvent {
    FocusPane(TileId),
    ToggleMoveMode(TileId),
    ToggleBroadcastSelectMode(TileId),
    StartEditingTitle(TileId),
    FinishEditingTitle(TileId, String),
    CancelEditingTitle(TileId),
    OpenPreferences,
    MinimizeWindow,
    ToggleMaximizeWindow,
    ClosePane(TileId),
    OpenContextMenu {
        tile_id: TileId,
        x_bits: u64,
        y_bits: u64,
    },
}

/// Computes the effective `(bg, fg)` colors for a pane's titlebar given its context and theme.
pub fn resolve_titlebar_colors(ctx: &PaneContext, settings: &ThemeSettings) -> (RgbColor, RgbColor) {
    let fg = if ctx.is_active {
        settings.effective_title_active_fg()
    } else {
        settings.effective_title_inactive_fg()
    };

    let bg = if ctx.broadcast_group >= 1 {
        broadcast_group_title_bg(ctx.broadcast_group, ctx.is_active)
    } else if ctx.is_active {
        settings.effective_title_active_bg()
    } else {
        settings.effective_title_inactive_bg()
    };

    (bg, fg)
}

pub fn lighten_rgb(c: RgbColor, delta: u8) -> RgbColor {
    RgbColor(
        c.0.saturating_add(delta),
        c.1.saturating_add(delta),
        c.2.saturating_add(delta),
    )
}

pub fn darken_rgb(c: RgbColor, delta: u8) -> RgbColor {
    RgbColor(
        c.0.saturating_sub(delta),
        c.1.saturating_sub(delta),
        c.2.saturating_sub(delta),
    )
}

pub fn blend_rgb(a: RgbColor, b: RgbColor, t: f32) -> RgbColor {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    let r = (a.0 as f32 * inv + b.0 as f32 * t).round().clamp(0.0, 255.0) as u8;
    let g = (a.1 as f32 * inv + b.1 as f32 * t).round().clamp(0.0, 255.0) as u8;
    let bl = (a.2 as f32 * inv + b.2 as f32 * t).round().clamp(0.0, 255.0) as u8;
    RgbColor(r, g, bl)
}

/// Pure state transition helper for `[ ✥ ]` grip clicks (used by both UI and unit tests).
pub fn next_mode_on_grip_click(
    current: InteractionMode,
    tile_id: TileId,
    right_click: bool,
) -> InteractionMode {
    if right_click {
        if current == InteractionMode::SelectingBroadcast(tile_id) {
            InteractionMode::Normal
        } else {
            InteractionMode::SelectingBroadcast(tile_id)
        }
    } else if current == InteractionMode::MovingPane(tile_id) {
        InteractionMode::Normal
    } else {
        InteractionMode::MovingPane(tile_id)
    }
}

pub struct TitlebarWidget {
    pub root: gtk4::CenterBox,
    tile_id: TileId,
    scope_class: String,
    grip_btn: gtk4::Button,
    badge_label: gtk4::Label,
    title_stack: gtk4::Stack,
    title_label: gtk4::Label,
    title_entry: gtk4::Entry,
    min_btn: gtk4::Button,
    max_btn: gtk4::Button,
    css_provider: gtk4::CssProvider,
    last_css: RefCell<String>,
    current_title: Rc<RefCell<String>>,
    is_editing: Rc<Cell<bool>>,
}

impl Drop for TitlebarWidget {
    fn drop(&mut self) {
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_remove_provider_for_display(&display, &self.css_provider);
        }
    }
}

impl TitlebarWidget {
    pub fn new<F>(tile_id: TileId, initial_title: &str, on_event: F) -> Rc<Self>
    where
        F: Fn(TitlebarEvent) + 'static,
    {
        let on_event: Rc<dyn Fn(TitlebarEvent)> = Rc::new(on_event);
        let scope_class = format!("fwer-tb-id-{}", tile_id.0);
        let root = gtk4::CenterBox::new();
        root.set_height_request(20);
        root.set_shrink_center_last(true);
        root.add_css_class("fwer-titlebar");
        root.add_css_class(&scope_class);

        let css_provider = gtk4::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &css_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 10,
            );
        }

        // Left area: [ ✥ ] grip button + broadcast group badge
        let left_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        left_box.set_margin_start(4);
        left_box.set_valign(gtk4::Align::Center);

        let grip_btn = gtk4::Button::with_label("[ ✥ ]");
        grip_btn.add_css_class("fwer-tb-btn");
        grip_btn.add_css_class("fwer-tb-btn-grip");
        grip_btn.set_valign(gtk4::Align::Center);
        grip_btn.set_focusable(false);
        grip_btn.set_tooltip_text(Some(
            "Left-click: Move/Swap Pane | Right-click: Broadcast Group",
        ));

        {
            let cb = Rc::clone(&on_event);
            grip_btn.connect_clicked(move |_| {
                cb(TitlebarEvent::ToggleMoveMode(tile_id));
            });
        }
        {
            let right_click = gtk4::GestureClick::new();
            right_click.set_button(3);
            right_click.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let cb = Rc::clone(&on_event);
            right_click.connect_pressed(move |gesture, _n, _x, _y| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                cb(TitlebarEvent::ToggleBroadcastSelectMode(tile_id));
            });
            grip_btn.add_controller(right_click);
        }

        let badge_label = gtk4::Label::new(None);
        badge_label.add_css_class("fwer-broadcast-badge");
        badge_label.set_valign(gtk4::Align::Center);
        badge_label.set_visible(false);

        left_box.append(&grip_btn);
        left_box.append(&badge_label);
        root.set_start_widget(Some(&left_box));

        // Center area: centered title label / inline edit entry
        let title_stack = gtk4::Stack::new();
        title_stack.set_hhomogeneous(false);
        title_stack.set_vhomogeneous(false);
        title_stack.set_hexpand(true);
        title_stack.set_halign(gtk4::Align::Fill);
        title_stack.set_valign(gtk4::Align::Center);

        let center_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        center_box.set_hexpand(true);
        center_box.set_halign(gtk4::Align::Center);
        center_box.set_valign(gtk4::Align::Center);

        let title_label = gtk4::Label::new(Some(initial_title));
        title_label.add_css_class("fwer-tb-title");
        title_label.set_valign(gtk4::Align::Center);
        title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        center_box.append(&title_label);

        let title_entry = gtk4::Entry::new();
        title_entry.set_text(initial_title);
        title_entry.add_css_class("fwer-tb-entry");
        title_entry.set_valign(gtk4::Align::Center);
        title_entry.set_width_chars(4);
        title_entry.set_max_width_chars(24);
        title_entry.set_hexpand(true);

        title_stack.add_named(&center_box, Some("label"));
        title_stack.add_named(&title_entry, Some("entry"));
        title_stack.set_visible_child_name("label");
        root.set_center_widget(Some(&title_stack));

        // Right area: ⚙, — (top-right only), □ (top-right only), ×
        let right_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 3);
        right_box.set_margin_end(4);
        right_box.set_valign(gtk4::Align::Center);

        let prefs_btn = gtk4::Button::with_label("⚙");
        prefs_btn.add_css_class("fwer-tb-btn");
        prefs_btn.add_css_class("fwer-tb-btn-prefs");
        prefs_btn.set_valign(gtk4::Align::Center);
        prefs_btn.set_focusable(false);
        prefs_btn.set_tooltip_text(Some("Preferences"));
        {
            let cb = Rc::clone(&on_event);
            prefs_btn.connect_clicked(move |_| {
                cb(TitlebarEvent::OpenPreferences);
            });
        }

        let min_btn = gtk4::Button::with_label("—");
        min_btn.add_css_class("fwer-tb-btn");
        min_btn.add_css_class("fwer-tb-btn-min");
        min_btn.set_valign(gtk4::Align::Center);
        min_btn.set_focusable(false);
        min_btn.set_tooltip_text(Some("Minimize Window"));
        {
            let cb = Rc::clone(&on_event);
            min_btn.connect_clicked(move |_| {
                cb(TitlebarEvent::MinimizeWindow);
            });
        }

        let max_btn = gtk4::Button::with_label("□");
        max_btn.add_css_class("fwer-tb-btn");
        max_btn.add_css_class("fwer-tb-btn-max");
        max_btn.set_valign(gtk4::Align::Center);
        max_btn.set_focusable(false);
        max_btn.set_tooltip_text(Some("Maximize / Restore Window"));
        {
            let cb = Rc::clone(&on_event);
            max_btn.connect_clicked(move |_| {
                cb(TitlebarEvent::ToggleMaximizeWindow);
            });
        }

        let close_btn = gtk4::Button::with_label("×");
        close_btn.add_css_class("fwer-tb-btn");
        close_btn.add_css_class("fwer-tb-btn-close");
        close_btn.set_valign(gtk4::Align::Center);
        close_btn.set_focusable(false);
        close_btn.set_tooltip_text(Some("Close Pane"));
        {
            let cb = Rc::clone(&on_event);
            close_btn.connect_clicked(move |_| {
                cb(TitlebarEvent::ClosePane(tile_id));
            });
        }

        right_box.append(&prefs_btn);
        right_box.append(&min_btn);
        right_box.append(&max_btn);
        right_box.append(&close_btn);
        root.set_end_widget(Some(&right_box));

        let current_title = Rc::new(RefCell::new(initial_title.to_string()));
        let is_editing = Rc::new(Cell::new(false));

        let widget = Rc::new(Self {
            root,
            tile_id,
            scope_class,
            grip_btn,
            badge_label,
            title_stack,
            title_label,
            title_entry,
            min_btn,
            max_btn,
            css_provider,
            last_css: RefCell::new(String::new()),
            current_title,
            is_editing,
        });

        widget.setup_interactions(on_event);
        widget
    }

    fn setup_interactions(self: &Rc<Self>, on_event: Rc<dyn Fn(TitlebarEvent)>) {
        // Double-click on the title label -> inline edit
        {
            let click_lbl = gtk4::GestureClick::new();
            click_lbl.set_button(1);
            let w_weak = Rc::downgrade(self);
            let cb = Rc::clone(&on_event);
            click_lbl.connect_pressed(move |gesture, n_press, _x, _y| {
                if n_press == 2 {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    if let Some(w) = w_weak.upgrade() {
                        w.begin_inline_edit();
                        cb(TitlebarEvent::StartEditingTitle(w.tile_id));
                    }
                }
            });
            self.title_label.add_controller(click_lbl);
        }

        // Left-click / double-click / drag on the titlebar background -> focus, maximize toggle, or window move
        {
            let click_bg = gtk4::GestureClick::new();
            click_bg.set_button(1);
            let w_weak = Rc::downgrade(self);
            let cb = Rc::clone(&on_event);
            click_bg.connect_pressed(move |gesture, n_press, x, y| {
                let Some(w) = w_weak.upgrade() else {
                    return;
                };
                if w.is_editing.get() {
                    if let Some(entry_bounds) = w.title_entry.compute_bounds(&w.root)
                        && entry_bounds.contains_point(&gtk4::graphene::Point::new(x as f32, y as f32))
                    {
                        return;
                    }
                    let text = w.title_entry.text().to_string();
                    w.commit_inline_edit(&text);
                    cb(TitlebarEvent::FinishEditingTitle(w.tile_id, text));
                    cb(TitlebarEvent::FocusPane(w.tile_id));
                    return;
                }
                cb(TitlebarEvent::FocusPane(w.tile_id));

                // Check if click is inside the title label
                if let Some(bounds) = w.title_label.compute_bounds(&w.root) {
                    if bounds.contains_point(&gtk4::graphene::Point::new(x as f32, y as f32)) {
                        if n_press == 2 {
                            gesture.set_state(gtk4::EventSequenceState::Claimed);
                            w.begin_inline_edit();
                            cb(TitlebarEvent::StartEditingTitle(w.tile_id));
                        }
                        return;
                    }
                }

                if n_press == 2 {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    cb(TitlebarEvent::ToggleMaximizeWindow);
                }
            });
            self.root.add_controller(click_bg);
        }

        // Drag on titlebar center -> native window move
        {
            let drag = gtk4::GestureDrag::new();
            drag.set_button(1);
            let w_weak = Rc::downgrade(self);
            drag.connect_drag_update(move |gesture, offset_x, offset_y| {
                if offset_x.hypot(offset_y) < 4.0 {
                    return;
                }
                let Some(w) = w_weak.upgrade() else {
                    return;
                };
                if w.is_editing.get() {
                    return;
                }
                if let Some((start_x, start_y)) = gesture.start_point() {
                    if let Some(native) = w.root.native() {
                        if let Some(surface) = native.surface() {
                            if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
                                #[allow(deprecated)]
                                let (nx, ny) = w
                                    .root
                                    .translate_coordinates(&native, start_x, start_y)
                                    .unwrap_or((start_x, start_y));
                                if let Some(device) = gesture.device() {
                                    let timestamp = gesture.current_event_time();
                                    gesture.reset();
                                    toplevel.begin_move(
                                        &device,
                                        1,
                                        nx,
                                        ny,
                                        timestamp,
                                    );
                                }
                            }
                        }
                    }
                }
            });
            self.root.add_controller(drag);
        }

        // Right-click on titlebar -> Titlebar context menu
        {
            let right_click = gtk4::GestureClick::new();
            right_click.set_button(3);
            let tile_id = self.tile_id;
            let cb = Rc::clone(&on_event);
            right_click.connect_pressed(move |gesture, _n, x, y| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                cb(TitlebarEvent::OpenContextMenu {
                    tile_id,
                    x_bits: x.to_bits(),
                    y_bits: y.to_bits(),
                });
            });
            self.root.add_controller(right_click);
        }

        // Entry commit on Enter or Escape
        {
            let w_weak = Rc::downgrade(self);
            let cb = Rc::clone(&on_event);
            self.title_entry.connect_activate(move |entry| {
                if let Some(w) = w_weak.upgrade() {
                    let text = entry.text().to_string();
                    w.commit_inline_edit(&text);
                    cb(TitlebarEvent::FinishEditingTitle(w.tile_id, text));
                }
            });
        }
        {
            let key_ctrl = gtk4::EventControllerKey::new();
            let w_weak = Rc::downgrade(self);
            let cb = Rc::clone(&on_event);
            key_ctrl.connect_key_pressed(move |_ctrl, key, _code, _mods| {
                if key == gdk::Key::Escape {
                    if let Some(w) = w_weak.upgrade() {
                        w.cancel_inline_edit();
                        cb(TitlebarEvent::CancelEditingTitle(w.tile_id));
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            self.title_entry.add_controller(key_ctrl);
        }
        {
            let focus_ctrl = gtk4::EventControllerFocus::new();
            let w_weak = Rc::downgrade(self);
            let cb = Rc::clone(&on_event);
            focus_ctrl.connect_leave(move |_| {
                if let Some(w) = w_weak.upgrade() {
                    if w.is_editing.get() {
                        let text = w.title_entry.text().to_string();
                        w.commit_inline_edit(&text);
                        cb(TitlebarEvent::FinishEditingTitle(w.tile_id, text));
                    }
                }
            });
            self.title_entry.add_controller(focus_ctrl);
        }
    }

    pub fn begin_inline_edit(&self) {
        self.is_editing.set(true);
        self.title_entry.set_text(&self.current_title.borrow());
        self.title_stack.set_visible_child_name("entry");
        self.title_entry.grab_focus();
        self.title_entry.select_region(0, -1);
    }

    pub fn commit_inline_edit(&self, new_title: &str) {
        self.is_editing.set(false);
        let final_title = if new_title.trim().is_empty() {
            self.current_title.borrow().clone()
        } else {
            new_title.trim().to_string()
        };
        *self.current_title.borrow_mut() = final_title.clone();
        self.title_label.set_text(&final_title);
        self.title_stack.set_visible_child_name("label");
    }

    pub fn cancel_inline_edit(&self) {
        self.is_editing.set(false);
        self.title_entry.set_text(&self.current_title.borrow());
        self.title_stack.set_visible_child_name("label");
    }

    pub fn finish_editing_if_active(&self) -> Option<String> {
        if self.is_editing.get() {
            let text = self.title_entry.text().to_string();
            self.commit_inline_edit(&text);
            Some(text)
        } else {
            None
        }
    }

    pub fn set_title(&self, title: &str) {
        *self.current_title.borrow_mut() = title.to_string();
        self.title_label.set_text(title);
        if !self.is_editing.get() {
            self.title_entry.set_text(title);
        }
    }

    pub fn title(&self) -> String {
        self.current_title.borrow().clone()
    }

    pub fn update_appearance(&self, ctx: &PaneContext, settings: &ThemeSettings) {
        // Only the top-right pane shows the window minimize and maximize buttons (Section 10.4)
        self.min_btn.set_visible(ctx.is_top_right);
        self.max_btn.set_visible(ctx.is_top_right);

        // Broadcast badge
        if ctx.broadcast_group >= 1 {
            self.badge_label
                .set_text(&format!("[ {} ]", ctx.broadcast_group));
            self.badge_label.set_visible(true);
        } else {
            self.badge_label.set_visible(false);
        }

        // Grip button highlight when move or broadcast selection mode originates here
        if ctx.is_move_source() {
            self.grip_btn.add_css_class("fwer-grip-moving");
            self.grip_btn.remove_css_class("fwer-grip-broadcast");
        } else if ctx.is_broadcast_source() {
            self.grip_btn.add_css_class("fwer-grip-broadcast");
            self.grip_btn.remove_css_class("fwer-grip-moving");
        } else {
            self.grip_btn.remove_css_class("fwer-grip-moving");
            self.grip_btn.remove_css_class("fwer-grip-broadcast");
        }

        let (bg, fg) = resolve_titlebar_colors(ctx, settings);
        let bg_hex = format_rgb_hex(bg);
        let fg_hex = format_rgb_hex(fg);

        let frame_bg_hex = format_rgb_hex(settings.effective_frame_bg_rgb());
        let frame_border_hex = format_rgb_hex(settings.effective_frame_border_rgb());
        let btn_border = blend_rgb(bg, fg, if ctx.is_active { 0.45 } else { 0.35 });
        let btn_border_hex = format_rgb_hex(btn_border);
        let btn_hover_bg = blend_rgb(bg, fg, 0.22);
        let btn_hover_bg_hex = format_rgb_hex(btn_hover_bg);
        let btn_active_bg = darken_rgb(bg, 18);
        let btn_active_bg_hex = format_rgb_hex(btn_active_bg);

        let ui_font_px = settings.effective_ui_font_size();
        let title_font_px = (ui_font_px - 3.0).clamp(9.0, 20.0);
        let btn_font_px = (ui_font_px - 3.0).clamp(9.0, 18.0);
        let bar_h = (title_font_px.round() as i32 + 10).max(20);
        let btn_h = (bar_h - 6).max(14);
        self.root.set_height_request(bar_h);
        let font_family = settings.effective_font_family();
        let scope = &self.scope_class;

        let css = format!(
            ".{scope}.fwer-titlebar {{
                background-image: none;
                background-color: {bg_hex};
                color: {fg_hex};
                min-height: {bar_h}px;
                padding: 0 2px;
                border: 2px solid {frame_bg_hex};
                box-shadow: inset 0 0 0 1px {frame_border_hex};
                font-family: \"{font_family}\", monospace;
                font-size: {title_font_px}px;
                line-height: 1.0;
             }}
             .{scope} label {{
                color: {fg_hex};
                font-size: {title_font_px}px;
                line-height: 1.0;
             }}
             .{scope} .fwer-tb-title {{
                color: {fg_hex};
                font-weight: normal;
                font-size: {title_font_px}px;
                line-height: 1.0;
                text-shadow: none;
                padding: 0 4px;
             }}
             .{scope} button.fwer-tb-btn {{
                background-image: none;
                background-color: rgba(0, 0, 0, 0.16);
                color: {fg_hex};
                border: 1px solid {btn_border_hex};
                border-radius: 2px;
                box-shadow: none;
                min-height: {btn_h}px;
                min-width: 16px;
                padding: 0 3px;
                margin: 0;
                font-size: {btn_font_px}px;
                font-weight: normal;
                line-height: 1.0;
             }}
             .{scope} button.fwer-tb-btn label {{
                color: {fg_hex};
                font-size: {btn_font_px}px;
                font-weight: normal;
                line-height: 1.0;
                text-shadow: none;
             }}
             .{scope} button.fwer-tb-btn:hover {{
                background-image: none;
                background-color: {btn_hover_bg_hex};
                border-color: {fg_hex};
             }}
             .{scope} button.fwer-tb-btn:active {{
                background-image: none;
                background-color: {btn_active_bg_hex};
             }}
             .{scope} button.fwer-tb-btn.fwer-grip-moving {{
                background-image: none;
                background-color: rgb(250, 204, 21);
                color: #111111;
                border-color: #111111;
                font-weight: bold;
             }}
             .{scope} button.fwer-tb-btn.fwer-grip-moving label {{
                color: #111111;
                font-weight: bold;
             }}
             .{scope} button.fwer-tb-btn.fwer-grip-broadcast {{
                background-image: none;
                background-color: rgb(56, 189, 248);
                color: #111111;
                border-color: #111111;
                font-weight: bold;
             }}
             .{scope} button.fwer-tb-btn.fwer-grip-broadcast label {{
                color: #111111;
                font-weight: bold;
             }}
             .{scope} .fwer-broadcast-badge {{
                background-color: rgba(0, 0, 0, 0.28);
                color: {fg_hex};
                border: 1px solid {btn_border_hex};
                border-radius: 3px;
                font-size: {btn_font_px}px;
                font-weight: bold;
                line-height: 1.0;
                padding: 0 4px;
             }}
             .{scope} entry.fwer-tb-entry {{
                min-height: {btn_h}px;
                padding: 0 3px;
                border-radius: 2px;
                font-size: {title_font_px}px;
                line-height: 1.0;
             }}"
        );
        if *self.last_css.borrow() != css {
            *self.last_css.borrow_mut() = css.clone();
            self.css_provider.load_from_string(&css);
        }
    }
}

#[cfg(test)]
#[path = "test/titlebar_tests.rs"]
mod titlebar_tests;
