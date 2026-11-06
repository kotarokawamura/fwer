// Copyright (C) [2026/11/06] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

//! Empty (pass-through frame-only) panel implementation (`EmptyPaneWidget`).
//!
//! Separated from `src/ui/pane.rs` (`TerminalPaneWidget`) so additional panel
//! types can be added cleanly as independent modules.

use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::config::theme::{ThemeSettings, matches_gdk_shortcut};
use crate::ui::pane::{PaneEvent, SpecialModeKey};
use crate::ui::state::{InteractionMode, PaneContext, TileId};
use crate::ui::titlebar::{TitlebarEvent, TitlebarWidget};
use crate::ui::tree_ops::Direction;

/// Frame border thickness (in px) kept around the pass-through hole of an empty pane
/// so that the panel frame itself remains visible and interactive.
pub const EMPTY_PANE_FRAME_INSET: i32 = 2;

/// Computes the pass-through hole rectangle `(x, y, width, height)` in surface coordinates
/// for an empty pane's interior body area, inset by `frame_inset` and clamped to `(surf_w, surf_h)`.
pub fn compute_pass_through_hole_rect(
    bounds_x: f32,
    bounds_y: f32,
    bounds_w: f32,
    bounds_h: f32,
    surf_w: i32,
    surf_h: i32,
    frame_inset: i32,
) -> Option<(i32, i32, i32, i32)> {
    if surf_w <= 0 || surf_h <= 0 || bounds_w <= 0.0 || bounds_h <= 0.0 {
        return None;
    }
    let inset = frame_inset.max(0);
    let raw_x1 = (bounds_x.round() as i32) + inset;
    let raw_y1 = (bounds_y.round() as i32) + inset;
    let raw_x2 = ((bounds_x + bounds_w).round() as i32) - inset;
    let raw_y2 = ((bounds_y + bounds_h).round() as i32) - inset;

    let x1 = raw_x1.clamp(0, surf_w);
    let y1 = raw_y1.clamp(0, surf_h);
    let x2 = raw_x2.clamp(0, surf_w);
    let y2 = raw_y2.clamp(0, surf_h);

    let w = x2 - x1;
    let h = y2 - y1;
    if w > 0 && h > 0 {
        Some((x1, y1, w, h))
    } else {
        None
    }
}

/// An empty panel that renders only its titlebar and outer frame while leaving its
/// interior completely transparent and input-pass-through so the user can interact
/// directly with applications behind the window.
pub struct EmptyPaneWidget {
    pub tile_id: TileId,
    pub root: gtk4::Box,
    pub titlebar: Rc<TitlebarWidget>,
    pub body_frame: gtk4::Box,
    pub pass_through_area: gtk4::Box,
    pub highlight_frame: gtk4::Box,
    current_mode: Rc<Cell<InteractionMode>>,
    is_hovered: Rc<Cell<bool>>,
    is_kb_target: Rc<Cell<bool>>,
    is_opacity_source: Rc<Cell<bool>>,
    opacity_confirmed: Rc<Cell<bool>>,
    initial_working_dir: RefCell<Option<PathBuf>>,
    settings_cache: Rc<RefCell<ThemeSettings>>,
}

impl EmptyPaneWidget {
    pub fn new<F>(
        tile_id: TileId,
        initial_title: &str,
        working_dir: Option<&Path>,
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
        root.add_css_class("fwer-empty-pane");

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

        let body_frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body_frame.set_hexpand(true);
        body_frame.set_vexpand(true);
        body_frame.add_css_class("fwer-empty-pane-body");

        let pass_through_area = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        pass_through_area.set_hexpand(true);
        pass_through_area.set_vexpand(true);
        pass_through_area.set_focusable(true);
        pass_through_area.set_can_focus(true);
        pass_through_area.add_css_class("fwer-empty-pane-hole");

        body_frame.append(&pass_through_area);
        inner_vbox.append(&body_frame);
        pane_overlay.set_child(Some(&inner_vbox));

        let highlight_frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        highlight_frame.set_hexpand(true);
        highlight_frame.set_vexpand(true);
        highlight_frame.set_can_target(false);
        highlight_frame.add_css_class("fwer-pane-highlight-frame");
        pane_overlay.add_overlay(&highlight_frame);

        root.append(&pane_overlay);

        let pane_rc = Rc::new(Self {
            tile_id,
            root,
            titlebar,
            body_frame,
            pass_through_area: pass_through_area.clone(),
            highlight_frame,
            current_mode: Rc::new(Cell::new(InteractionMode::Normal)),
            is_hovered: Rc::new(Cell::new(false)),
            is_kb_target: Rc::new(Cell::new(false)),
            is_opacity_source: Rc::new(Cell::new(false)),
            opacity_confirmed: Rc::new(Cell::new(false)),
            initial_working_dir: RefCell::new(working_dir.map(|p| p.to_path_buf())),
            settings_cache: Rc::new(RefCell::new(settings.clone())),
        });

        pane_rc.setup_controllers(on_event);
        pane_rc
    }

    fn setup_controllers(self: &Rc<Self>, on_event: Rc<dyn Fn(PaneEvent)>) {
        // 1. Click controller on body_frame (active during MovingPane / Broadcast / Resize / Opacity modes
        //    or when clicking the frame border itself)
        {
            let click = gtk4::GestureClick::new();
            click.set_button(0);
            click.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let pane_weak = Rc::downgrade(self);
            let cb = Rc::clone(&on_event);
            click.connect_pressed(move |gesture, _n_press, x, y| {
                let Some(pane) = pane_weak.upgrade() else {
                    return;
                };
                let mode = pane.current_mode.get();
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
                        cb(PaneEvent::ClickedInSpecialMode {
                            tile_id: pane.tile_id,
                            keep_selecting,
                        });
                    }
                    _ if mode.is_special_mode() => {
                        gesture.set_state(gtk4::EventSequenceState::Claimed);
                        cb(PaneEvent::CancelSpecialMode);
                    }
                    InteractionMode::EditingTitle(_) | InteractionMode::Normal => {
                        cb(PaneEvent::Titlebar(TitlebarEvent::FocusPane(pane.tile_id)));
                        if btn == 3 && mode == InteractionMode::Normal {
                            gesture.set_state(gtk4::EventSequenceState::Claimed);
                            cb(PaneEvent::OpenPaneContextMenu {
                                tile_id: pane.tile_id,
                                x,
                                y,
                            });
                        }
                    }
                    _ => {}
                }
            });
            self.body_frame.add_controller(click);
        }

        // 2. Keyboard controller so shortcuts (split, close, focus next/prev, move, resize)
        //    work when the empty pane is focused
        {
            let key_ctrl = gtk4::EventControllerKey::new();
            key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let pane_weak = Rc::downgrade(self);
            let cb = Rc::clone(&on_event);
            key_ctrl.connect_key_pressed(move |_ctrl, key, _keycode, mods| {
                let Some(pane) = pane_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let mode = pane.current_mode.get();
                let sc = pane.settings_cache.borrow().shortcuts.clone();

                if let Some(toggle_spec) = mode.toggle_shortcut(&sc) {
                    if matches_gdk_shortcut(toggle_spec, key, mods) {
                        cb(PaneEvent::CancelSpecialMode);
                        return glib::Propagation::Stop;
                    }

                    let keep = mods.contains(gdk::ModifierType::CONTROL_MASK)
                        || mods.contains(gdk::ModifierType::SHIFT_MASK)
                        || mods.contains(gdk::ModifierType::SUPER_MASK);

                    let special_key = match key {
                        gdk::Key::Escape | gdk::Key::q | gdk::Key::Q => {
                            Some(SpecialModeKey::Cancel)
                        }
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
                        gdk::Key::minus | gdk::Key::KP_Subtract => {
                            Some(SpecialModeKey::AdjustOpacity(-1))
                        }
                        _ => None,
                    };

                    if let Some(sk) = special_key {
                        if sk == SpecialModeKey::Cancel {
                            cb(PaneEvent::CancelSpecialMode);
                        } else {
                            cb(PaneEvent::SpecialModeKeyInput {
                                tile_id: pane.tile_id,
                                key: sk,
                            });
                        }
                    }
                    return glib::Propagation::Stop;
                }

                if mode != InteractionMode::Normal {
                    return glib::Propagation::Stop;
                }

                if let Some(action) = sc.match_pane_shortcut(key, mods) {
                    cb(PaneEvent::ShortcutTriggered {
                        tile_id: pane.tile_id,
                        action,
                    });
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            });
            self.pass_through_area.add_controller(key_ctrl);
        }

        // 3. Hover tracking for target highlight in MovingPane / SelectingBroadcast modes
        {
            let motion = gtk4::EventControllerMotion::new();
            let pane_weak_enter = Rc::downgrade(self);
            motion.connect_enter(move |_, _, _| {
                if let Some(pane) = pane_weak_enter.upgrade() {
                    pane.is_hovered.set(true);
                    pane.refresh_highlight_frame();
                }
            });
            let pane_weak_motion = Rc::downgrade(self);
            motion.connect_motion(move |_, _, _| {
                if let Some(pane) = pane_weak_motion.upgrade()
                    && !pane.is_hovered.get()
                {
                    pane.is_hovered.set(true);
                    pane.refresh_highlight_frame();
                }
            });
            let pane_weak_leave = Rc::downgrade(self);
            motion.connect_leave(move |_| {
                if let Some(pane) = pane_weak_leave.upgrade() {
                    pane.is_hovered.set(false);
                    pane.refresh_highlight_frame();
                }
            });
            self.root.add_controller(motion);
        }
    }

    pub fn pass_through_widget(&self) -> &gtk4::Widget {
        self.pass_through_area.upcast_ref::<gtk4::Widget>()
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

    pub fn refresh_highlight_frame(&self) {
        let mode = self.current_mode.get();

        for &cls in InteractionMode::ALL_SOURCE_CSS_CLASSES {
            self.highlight_frame.remove_css_class(cls);
        }
        self.highlight_frame
            .remove_css_class("fwer-pane-target-hover");

        if matches!(mode, InteractionMode::AdjustingOpacity(_)) {
            if self.is_opacity_source.get() {
                self.highlight_frame.add_css_class("fwer-pane-opacity-source");
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

    pub fn grab_terminal_focus(&self) {
        self.pass_through_area.grab_focus();
    }

    pub fn finish_title_edit_if_active(&self) -> Option<String> {
        self.titlebar.finish_editing_if_active()
    }

    pub fn current_working_dir(&self) -> Option<PathBuf> {
        self.initial_working_dir.borrow().clone()
    }

    pub fn update_context_and_theme(&self, ctx: &PaneContext, settings: &ThemeSettings) {
        self.current_mode.set(ctx.mode);
        *self.settings_cache.borrow_mut() = settings.clone();
        self.titlebar.set_read_only(false);
        self.titlebar.update_appearance(ctx, settings);
        self.refresh_highlight_frame();
    }
}

/// Dynamically sets or clears `_NET_WM_STATE_ABOVE` on an X11 `gdk::Surface` via `dlsym`
/// so that when an empty pass-through panel is present and the user clicks the underlying
/// application window behind `fwer`, the window manager does not raise the background
/// application over `fwer`'s frame.
pub fn set_x11_surface_keep_above(surface: &gdk::Surface, keep_above: bool) {
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct XClientMessageEvent {
        type_: libc::c_int,
        serial: libc::c_ulong,
        send_event: libc::c_int,
        display: *mut libc::c_void,
        window: libc::c_ulong,
        message_type: libc::c_ulong,
        format: libc::c_int,
        data: [libc::c_long; 5],
    }

    #[repr(C)]
    union XEvent {
        client: XClientMessageEvent,
        pad: [libc::c_long; 24],
    }

    unsafe {
        let sym = |name: &str| -> *mut libc::c_void {
            let Ok(cstr) = CString::new(name) else {
                return std::ptr::null_mut();
            };
            libc::dlsym(libc::RTLD_DEFAULT, cstr.as_ptr())
        };

        let get_xid_ptr = sym("gdk_x11_surface_get_xid");
        let get_xdisplay_ptr = sym("gdk_x11_display_get_xdisplay");
        let intern_atom_ptr = sym("XInternAtom");
        let default_root_ptr = sym("XDefaultRootWindow");
        let send_event_ptr = sym("XSendEvent");
        let flush_ptr = sym("XFlush");

        if get_xid_ptr.is_null()
            || get_xdisplay_ptr.is_null()
            || intern_atom_ptr.is_null()
            || default_root_ptr.is_null()
            || send_event_ptr.is_null()
            || flush_ptr.is_null()
        {
            return;
        }

        // Verify the surface is actually a GdkX11Surface before calling gdk_x11_surface_get_xid
        let type_name_ptr = glib::gobject_ffi::g_type_name_from_instance(
            surface.as_ptr() as *mut glib::gobject_ffi::GTypeInstance
        );
        if type_name_ptr.is_null() {
            return;
        }
        let type_name = std::ffi::CStr::from_ptr(type_name_ptr).to_string_lossy();
        if !type_name.contains("X11") {
            return;
        }

        type GetXidFn = unsafe extern "C" fn(*mut libc::c_void) -> libc::c_ulong;
        type GetXDisplayFn = unsafe extern "C" fn(*mut libc::c_void) -> *mut libc::c_void;
        type XInternAtomFn =
            unsafe extern "C" fn(*mut libc::c_void, *const libc::c_char, libc::c_int) -> libc::c_ulong;
        type XDefaultRootWindowFn = unsafe extern "C" fn(*mut libc::c_void) -> libc::c_ulong;
        type XSendEventFn = unsafe extern "C" fn(
            *mut libc::c_void,
            libc::c_ulong,
            libc::c_int,
            libc::c_long,
            *mut XEvent,
        ) -> libc::c_int;
        type XFlushFn = unsafe extern "C" fn(*mut libc::c_void) -> libc::c_int;

        let get_xid: GetXidFn = std::mem::transmute(get_xid_ptr);
        let get_xdisplay: GetXDisplayFn = std::mem::transmute(get_xdisplay_ptr);
        let intern_atom: XInternAtomFn = std::mem::transmute(intern_atom_ptr);
        let default_root: XDefaultRootWindowFn = std::mem::transmute(default_root_ptr);
        let send_event: XSendEventFn = std::mem::transmute(send_event_ptr);
        let xflush: XFlushFn = std::mem::transmute(flush_ptr);

        let display_obj = surface.display();
        let xdisplay = get_xdisplay(display_obj.as_ptr() as *mut libc::c_void);
        let xid = get_xid(surface.as_ptr() as *mut libc::c_void);
        if xdisplay.is_null() || xid == 0 {
            return;
        }

        let wm_state_c = CString::new("_NET_WM_STATE").unwrap();
        let wm_above_c = CString::new("_NET_WM_STATE_ABOVE").unwrap();
        let wm_state = intern_atom(xdisplay, wm_state_c.as_ptr(), 0);
        let wm_above = intern_atom(xdisplay, wm_above_c.as_ptr(), 0);
        let root_win = default_root(xdisplay);

        let mut ev = XEvent { pad: [0; 24] };
        ev.client = XClientMessageEvent {
            type_: 33, // ClientMessage
            serial: 0,
            send_event: 1,
            display: xdisplay,
            window: xid,
            message_type: wm_state,
            format: 32,
            data: [
                if keep_above { 1 } else { 0 }, // _NET_WM_STATE_ADD = 1, _NET_WM_STATE_REMOVE = 0
                wm_above as libc::c_long,
                0,
                1, // source indication: normal application
                0,
            ],
        };

        // SubstructureRedirectMask (1 << 20) | SubstructureNotifyMask (1 << 19)
        let mask: libc::c_long = (1 << 20) | (1 << 19);
        send_event(xdisplay, root_win, 0, mask, &mut ev);
        xflush(xdisplay);
    }
}
