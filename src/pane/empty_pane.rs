// Copyright (C) [2026/11/06] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

//! Empty (pass-through frame-only) panel implementation (`EmptyPaneWidget`).

use std::cell::RefCell;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::Widget;

use super::interaction::PaneInteraction;
use super::types::{PaneEvent, PaneShortcutAction};
use crate::config::theme::{matches_gdk_shortcut, ThemeSettings};
use crate::ui::state::{PaneContext, TileId};
use crate::ui::titlebar::TitlebarWidget;

/// Returns `true` when the key press is `Ctrl+D` (`C-d`), which closes an empty pane
/// just like EOF exits a shell pane.
pub fn is_empty_pane_eof_close_key(key: gdk::Key, mods: gdk::ModifierType) -> bool {
    matches_gdk_shortcut("Ctrl+D", key, mods)
}

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

pub struct EmptyPaneWidget {
    pub tile_id: TileId,
    pub root: gtk4::Box,
    pub titlebar: Rc<TitlebarWidget>,
    pub body_frame: gtk4::Box,
    pub pass_through_area: gtk4::DrawingArea,
    pub highlight_frame: gtk4::Box,
    interaction: Rc<PaneInteraction>,
    initial_working_dir: RefCell<Option<PathBuf>>,
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

        let pass_through_area = gtk4::DrawingArea::new();
        pass_through_area.set_hexpand(true);
        pass_through_area.set_vexpand(true);
        pass_through_area.set_focusable(true);
        pass_through_area.set_can_focus(true);
        pass_through_area.add_css_class("fwer-empty-pane-hole");

        body_frame.append(&pass_through_area);
        inner_vbox.append(&body_frame);
        pane_overlay.set_child(Some(&inner_vbox));

        let interaction = PaneInteraction::new(tile_id, &root, &pane_overlay, settings, on_event);
        let highlight_frame = interaction.highlight_frame.clone();

        root.append(&pane_overlay);

        let pane_rc = Rc::new(Self {
            tile_id,
            root,
            titlebar,
            body_frame,
            pass_through_area,
            highlight_frame,
            interaction,
            initial_working_dir: RefCell::new(working_dir.map(|p| p.to_path_buf())),
        });

        pane_rc.setup_controllers();
        pane_rc
    }

    fn setup_controllers(self: &Rc<Self>) {
        self.interaction.attach_click_controller(&self.body_frame);

        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let pane_weak = Rc::downgrade(self);
        key_ctrl.connect_key_pressed(move |_ctrl, key, _keycode, mods| {
            let Some(pane) = pane_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if pane.handle_key_press(key, mods) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.pass_through_area.add_controller(key_ctrl);
    }

    pub fn pass_through_widget(&self) -> &Widget {
        self.pass_through_area.upcast_ref::<Widget>()
    }

    pub fn set_keyboard_target(&self, targeted: bool) {
        self.interaction.set_keyboard_target(targeted);
    }

    pub fn set_opacity_highlight(&self, is_source: bool, confirmed: bool) {
        self.interaction.set_opacity_highlight(is_source, confirmed);
    }

    pub fn refresh_highlight_frame(&self) {
        self.interaction.refresh_highlight_frame();
    }

    pub fn grab_terminal_focus(&self) {
        self.pass_through_area.grab_focus();
        let area = self.pass_through_area.clone();
        glib::idle_add_local_once(move || {
            if area.is_mapped() {
                area.grab_focus();
            }
        });
    }

    pub fn handle_key_press(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        if self.interaction.handle_common_key_press(key, mods) {
            return true;
        }

        if is_empty_pane_eof_close_key(key, mods) {
            self.interaction.emit(PaneEvent::ShortcutTriggered {
                tile_id: self.tile_id,
                action: PaneShortcutAction::ClosePane,
            });
            return true;
        }

        // Prevent non-shortcut keys from leaking to inactive terminal panes when an empty pane is active
        key != gdk::Key::Escape
    }

    pub fn finish_title_edit_if_active(&self) -> Option<String> {
        self.titlebar.finish_editing_if_active()
    }

    pub fn current_working_dir(&self) -> Option<PathBuf> {
        self.initial_working_dir.borrow().clone()
    }

    pub fn update_context_and_theme(&self, ctx: &PaneContext, settings: &ThemeSettings) {
        self.titlebar.set_read_only(false);
        self.titlebar.update_appearance(ctx, settings);
        self.interaction.update_context_and_settings(ctx, settings);
    }
}

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
        type XInternAtomFn = unsafe extern "C" fn(
            *mut libc::c_void,
            *const libc::c_char,
            libc::c_int,
        ) -> libc::c_ulong;
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
            type_: 33,
            serial: 0,
            send_event: 1,
            display: xdisplay,
            window: xid,
            message_type: wm_state,
            format: 32,
            data: [
                if keep_above { 1 } else { 0 },
                wm_above as libc::c_long,
                0,
                1,
                0,
            ],
        };

        let mask: libc::c_long = (1 << 20) | (1 << 19);
        send_event(xdisplay, root_win, 0, mask, &mut ev);
        xflush(xdisplay);
    }
}
