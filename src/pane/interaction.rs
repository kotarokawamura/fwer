// Copyright (C) [2026/11/13] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

//! Shared panel interaction state (mode, hover/keyboard targets, highlight frame)
//! and common input controllers (special-mode keys, shortcuts, click, and hover).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use super::types::{PaneEvent, SpecialModeKey};
use crate::config::theme::{ThemeSettings, matches_gdk_shortcut};
use crate::ui::state::{InteractionMode, PaneContext, TileId};
use crate::ui::titlebar::TitlebarEvent;
use crate::ui::tree_ops::Direction;

/// Maps a key press during a special interaction mode (`MovingPane`, `ResizingPane`,
/// `SelectingBroadcast`, `AdjustingOpacity`) to a [`SpecialModeKey`].
pub fn map_special_mode_key(key: gdk::Key, mods: gdk::ModifierType) -> Option<SpecialModeKey> {
    let keep = mods.contains(gdk::ModifierType::CONTROL_MASK)
        || mods.contains(gdk::ModifierType::SHIFT_MASK)
        || mods.contains(gdk::ModifierType::SUPER_MASK);

    match key {
        gdk::Key::Escape | gdk::Key::q | gdk::Key::Q => Some(SpecialModeKey::Cancel),
        gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
            if keep {
                Some(SpecialModeKey::KeepSelecting)
            } else {
                Some(SpecialModeKey::Confirm)
            }
        }
        gdk::Key::Left | gdk::Key::KP_Left | gdk::Key::h | gdk::Key::H => {
            Some(SpecialModeKey::Move(Direction::Left))
        }
        gdk::Key::Right | gdk::Key::KP_Right | gdk::Key::l | gdk::Key::L => {
            Some(SpecialModeKey::Move(Direction::Right))
        }
        gdk::Key::Up | gdk::Key::KP_Up | gdk::Key::k | gdk::Key::K => {
            Some(SpecialModeKey::Move(Direction::Up))
        }
        gdk::Key::Down | gdk::Key::KP_Down | gdk::Key::j | gdk::Key::J => {
            Some(SpecialModeKey::Move(Direction::Down))
        }
        gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => {
            Some(SpecialModeKey::AdjustOpacity(1))
        }
        gdk::Key::minus | gdk::Key::KP_Subtract => Some(SpecialModeKey::AdjustOpacity(-1)),
        _ => None,
    }
}

/// Manages per-pane interaction state, highlight frame CSS, and shared input controllers.
pub struct PaneInteraction {
    pub tile_id: TileId,
    pub highlight_frame: gtk4::Box,
    current_mode: Cell<InteractionMode>,
    is_hovered: Cell<bool>,
    is_kb_target: Cell<bool>,
    is_opacity_source: Cell<bool>,
    opacity_confirmed: Cell<bool>,
    settings_cache: RefCell<ThemeSettings>,
    on_event: Rc<dyn Fn(PaneEvent)>,
}

impl PaneInteraction {
    pub fn new(
        tile_id: TileId,
        root: &gtk4::Box,
        pane_overlay: &gtk4::Overlay,
        settings: &ThemeSettings,
        on_event: Rc<dyn Fn(PaneEvent)>,
    ) -> Rc<Self> {
        let highlight_frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        highlight_frame.set_hexpand(true);
        highlight_frame.set_vexpand(true);
        highlight_frame.set_can_target(false);
        highlight_frame.add_css_class("fwer-pane-highlight-frame");
        pane_overlay.add_overlay(&highlight_frame);

        let state = Rc::new(Self {
            tile_id,
            highlight_frame,
            current_mode: Cell::new(InteractionMode::Normal),
            is_hovered: Cell::new(false),
            is_kb_target: Cell::new(false),
            is_opacity_source: Cell::new(false),
            opacity_confirmed: Cell::new(false),
            settings_cache: RefCell::new(settings.clone()),
            on_event,
        });

        state.attach_hover_controller(root);
        state
    }

    pub fn emit(&self, event: PaneEvent) {
        (self.on_event)(event);
    }

    pub fn mode(&self) -> InteractionMode {
        self.current_mode.get()
    }

    pub fn settings(&self) -> ThemeSettings {
        self.settings_cache.borrow().clone()
    }

    pub fn set_keyboard_target(&self, targeted: bool) {
        self.is_kb_target.set(targeted);
        self.refresh_highlight_frame();
    }

    pub fn set_opacity_highlight(&self, is_source: bool, confirmed: bool) {
        self.is_opacity_source.set(is_source);
        self.opacity_confirmed.set(confirmed);
        self.refresh_highlight_frame();
    }

    pub fn update_context_and_settings(&self, ctx: &PaneContext, settings: &ThemeSettings) {
        self.current_mode.set(ctx.mode);
        *self.settings_cache.borrow_mut() = settings.clone();
        self.refresh_highlight_frame();
    }

    pub fn refresh_highlight_frame(&self) {
        let mode = self.current_mode.get();

        for &cls in InteractionMode::ALL_SOURCE_CSS_CLASSES {
            self.highlight_frame.remove_css_class(cls);
        }
        self.highlight_frame
            .remove_css_class("fwer-pane-target-hover");

        if matches!(mode, InteractionMode::AdjustingOpacity(_)) {
            if self.is_opacity_source.get() {
                self.highlight_frame
                    .add_css_class("fwer-pane-opacity-source");
            } else if !self.opacity_confirmed.get()
                && (self.is_hovered.get() || self.is_kb_target.get())
            {
                self.highlight_frame.add_css_class("fwer-pane-target-hover");
            }
            return;
        }

        if let Some(cls) = mode.source_css_class(self.tile_id) {
            self.highlight_frame.add_css_class(cls);
        } else if mode.click_selects_pane() && (self.is_hovered.get() || self.is_kb_target.get()) {
            self.highlight_frame.add_css_class("fwer-pane-target-hover");
        }
    }

    /// Attaches the capture-phase mouse click controller for special modes, pane focus,
    /// and right-click context menu.
    pub fn attach_click_controller(self: &Rc<Self>, target_widget: &impl IsA<gtk4::Widget>) {
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let state_weak = Rc::downgrade(self);
        click.connect_pressed(move |gesture, _n_press, x, y| {
            let Some(st) = state_weak.upgrade() else {
                return;
            };
            let mode = st.current_mode.get();
            let btn = gesture.current_button();
            let mods = gesture.current_event_state();
            let keep_selecting = mods.contains(gdk::ModifierType::CONTROL_MASK)
                || mods.contains(gdk::ModifierType::SUPER_MASK)
                || mods.contains(gdk::ModifierType::META_MASK);

            match mode {
                InteractionMode::BlockedByModal => {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                }
                _ if mode.click_selects_pane() => {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    st.emit(PaneEvent::ClickedInSpecialMode {
                        tile_id: st.tile_id,
                        keep_selecting,
                    });
                }
                _ if mode.is_special_mode() => {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    st.emit(PaneEvent::CancelSpecialMode);
                }
                InteractionMode::EditingTitle(_) | InteractionMode::Normal => {
                    st.emit(PaneEvent::Titlebar(TitlebarEvent::FocusPane(st.tile_id)));
                    if btn == 3 && mode == InteractionMode::Normal {
                        gesture.set_state(gtk4::EventSequenceState::Claimed);
                        st.emit(PaneEvent::OpenPaneContextMenu {
                            tile_id: st.tile_id,
                            x,
                            y,
                        });
                    }
                }
                _ => {}
            }
        });
        target_widget.add_controller(click);
    }

    /// Attaches hover motion tracking to update target border highlights during special modes.
    fn attach_hover_controller(self: &Rc<Self>, root_widget: &impl IsA<gtk4::Widget>) {
        let motion = gtk4::EventControllerMotion::new();
        let weak_enter = Rc::downgrade(self);
        motion.connect_enter(move |_, _, _| {
            if let Some(st) = weak_enter.upgrade() {
                st.is_hovered.set(true);
                st.refresh_highlight_frame();
            }
        });
        let weak_motion = Rc::downgrade(self);
        motion.connect_motion(move |_, _, _| {
            if let Some(st) = weak_motion.upgrade()
                && !st.is_hovered.get()
            {
                st.is_hovered.set(true);
                st.refresh_highlight_frame();
            }
        });
        let weak_leave = Rc::downgrade(self);
        motion.connect_leave(move |_| {
            if let Some(st) = weak_leave.upgrade() {
                st.is_hovered.set(false);
                st.refresh_highlight_frame();
            }
        });
        root_widget.add_controller(motion);
    }

    /// Processes special-mode key bindings, modal/title-editing blocks, and pane shortcuts.
    /// Returns `true` if the key press was consumed, or `false` if panel-specific key handling should run.
    pub fn handle_common_key_press(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        let mode = self.current_mode.get();
        let sc = self.settings_cache.borrow().shortcuts.clone();

        if let Some(toggle_spec) = mode.toggle_shortcut(&sc) {
            if matches_gdk_shortcut(toggle_spec, key, mods) {
                self.emit(PaneEvent::CancelSpecialMode);
                return true;
            }

            if let Some(sk) = map_special_mode_key(key, mods) {
                if sk == SpecialModeKey::Cancel {
                    self.emit(PaneEvent::CancelSpecialMode);
                } else {
                    self.emit(PaneEvent::SpecialModeKeyInput {
                        tile_id: self.tile_id,
                        key: sk,
                    });
                }
            }
            return true;
        }

        if mode != InteractionMode::Normal {
            return true;
        }

        if let Some(action) = sc.match_pane_shortcut(key, mods) {
            self.emit(PaneEvent::ShortcutTriggered {
                tile_id: self.tile_id,
                action,
            });
            return true;
        }

        false
    }

    /// Handles vertical scroll during `AdjustingOpacity` mode, returning `Stop` if consumed.
    pub fn handle_opacity_scroll(&self, dy: f64) -> glib::Propagation {
        let mode = self.current_mode.get();
        if matches!(mode, InteractionMode::AdjustingOpacity(_)) {
            if dy.abs() > 1e-4 {
                let delta = if dy.abs() >= 0.9 {
                    if dy < 0.0 { 0.05 } else { -0.05 }
                } else {
                    (-dy as f32 * 0.05).clamp(-0.05, 0.05)
                };
                self.emit(PaneEvent::OpacityScroll {
                    tile_id: self.tile_id,
                    delta,
                });
            }
            return glib::Propagation::Stop;
        }
        if mode != InteractionMode::Normal {
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }
}
