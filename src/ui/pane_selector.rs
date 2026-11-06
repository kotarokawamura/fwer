// Copyright (C) [2026/11/06] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

//! Center-of-screen panel type selector (`PaneSelectorWidget`) shown when splitting
//! a pane horizontally or vertically (when `select_pane_type_on_split` is enabled).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::i18n::tr;
use crate::ui::pane::PaneKind;
use crate::ui::state::TileId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingSplitRequest {
    pub target_id: TileId,
    pub horizontal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorKeyAction {
    MovePrev,
    MoveNext,
    Confirm,
    Cancel,
}

/// Maps a GDK key press to a [`SelectorKeyAction`] while the panel type selector is open.
///
/// Supports:
/// - `J` / `j`, `Down` / `KP_Down`, `Right` / `KP_Right` -> [`SelectorKeyAction::MoveNext`]
/// - `K` / `k`, `Up` / `KP_Up`, `Left` / `KP_Left` -> [`SelectorKeyAction::MovePrev`]
/// - `Tab` / `ISO_Left_Tab` / `KP_Tab`, `Return` / `KP_Enter`, `space` -> [`SelectorKeyAction::Confirm`]
/// - `Escape` / `q` / `Q` -> [`SelectorKeyAction::Cancel`]
pub fn map_selector_key(key: gdk::Key) -> Option<SelectorKeyAction> {
    match key {
        gdk::Key::j
        | gdk::Key::J
        | gdk::Key::Down
        | gdk::Key::KP_Down
        | gdk::Key::Right
        | gdk::Key::KP_Right => Some(SelectorKeyAction::MoveNext),

        gdk::Key::k
        | gdk::Key::K
        | gdk::Key::Up
        | gdk::Key::KP_Up
        | gdk::Key::Left
        | gdk::Key::KP_Left => Some(SelectorKeyAction::MovePrev),

        gdk::Key::Tab
        | gdk::Key::ISO_Left_Tab
        | gdk::Key::KP_Tab
        | gdk::Key::Return
        | gdk::Key::KP_Enter
        | gdk::Key::space => Some(SelectorKeyAction::Confirm),

        gdk::Key::Escape | gdk::Key::q | gdk::Key::Q => Some(SelectorKeyAction::Cancel),

        _ => None,
    }
}

/// Computes the next wrapped index in `0..len` given a signed `delta` (`-1` or `+1`).
pub fn next_selector_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let cur = (current % len) as isize;
    let l = len as isize;
    (((cur + delta) % l + l) % l) as usize
}

pub struct PaneSelectorWidget {
    pub backdrop: gtk4::Box,
    pub card: gtk4::Box,
    title_label: gtk4::Label,
    hint_label: gtk4::Label,
    items_box: gtk4::Box,
    row_widgets: RefCell<Vec<gtk4::Box>>,
    items: Vec<PaneKind>,
    selected_index: Cell<usize>,
    pending_request: Cell<Option<PendingSplitRequest>>,
    on_confirm: Rc<dyn Fn(PendingSplitRequest, PaneKind)>,
    on_cancel: Rc<dyn Fn()>,
}

impl PaneSelectorWidget {
    pub fn new(
        on_confirm: impl Fn(PendingSplitRequest, PaneKind) + 'static,
        on_cancel: impl Fn() + 'static,
    ) -> Rc<Self> {
        let backdrop = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        backdrop.set_hexpand(true);
        backdrop.set_vexpand(true);
        backdrop.set_halign(gtk4::Align::Fill);
        backdrop.set_valign(gtk4::Align::Fill);
        backdrop.set_visible(false);
        backdrop.add_css_class("fwer-pane-selector-backdrop");

        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        card.set_halign(gtk4::Align::Center);
        card.set_valign(gtk4::Align::Center);
        card.set_width_request(310);
        card.set_focusable(true);
        card.set_can_focus(true);
        card.add_css_class("fwer-pane-selector-card");

        // Center the card vertically and horizontally inside the backdrop
        let v_center_spacer_top = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        v_center_spacer_top.set_vexpand(true);
        let v_center_spacer_bottom = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        v_center_spacer_bottom.set_vexpand(true);

        let title_label = gtk4::Label::new(None);
        title_label.set_halign(gtk4::Align::Start);
        title_label.add_css_class("fwer-pane-selector-title");
        card.append(&title_label);

        card.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let items_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        items_box.set_hexpand(true);
        card.append(&items_box);

        let hint_label = gtk4::Label::new(None);
        hint_label.set_halign(gtk4::Align::Center);
        hint_label.add_css_class("fwer-pane-selector-hint");
        card.append(&hint_label);

        backdrop.append(&v_center_spacer_top);
        backdrop.append(&card);
        backdrop.append(&v_center_spacer_bottom);

        let selector = Rc::new(Self {
            backdrop,
            card,
            title_label,
            hint_label,
            items_box,
            row_widgets: RefCell::new(Vec::new()),
            items: PaneKind::ALL.to_vec(),
            selected_index: Cell::new(0),
            pending_request: Cell::new(None),
            on_confirm: Rc::new(on_confirm),
            on_cancel: Rc::new(on_cancel),
        });

        selector.setup_controllers();
        selector.rebuild_rows();
        selector
    }

    fn setup_controllers(self: &Rc<Self>) {
        // Click outside the card on the backdrop cancels the selector
        {
            let click_bg = gtk4::GestureClick::new();
            click_bg.set_button(1);
            let sel_weak = Rc::downgrade(self);
            click_bg.connect_pressed(move |gesture, _n_press, x, y| {
                let Some(sel) = sel_weak.upgrade() else {
                    return;
                };
                if !sel.is_open() {
                    return;
                }
                if let Some(card_bounds) = sel.card.compute_bounds(&sel.backdrop)
                    && card_bounds.contains_point(&gtk4::graphene::Point::new(x as f32, y as f32))
                {
                    return;
                }
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                sel.cancel();
            });
            self.backdrop.add_controller(click_bg);
        }

        // Key controller directly on the card
        {
            let key_ctrl = gtk4::EventControllerKey::new();
            key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let sel_weak = Rc::downgrade(self);
            key_ctrl.connect_key_pressed(move |_ctrl, key, _code, _mods| {
                let Some(sel) = sel_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if sel.handle_key_press(key) {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
            self.card.add_controller(key_ctrl);
        }
    }

    fn rebuild_rows(self: &Rc<Self>) {
        while let Some(c) = self.items_box.first_child() {
            self.items_box.remove(&c);
        }
        self.row_widgets.borrow_mut().clear();

        for (idx, &kind) in self.items.iter().enumerate() {
            let row_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
            row_box.set_hexpand(true);
            row_box.add_css_class("fwer-pane-selector-item");

            let top_line = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let name_lbl = gtk4::Label::new(None);
            name_lbl.set_markup(&format!(
                "{}  <b>{}</b>",
                kind.icon(),
                glib::markup_escape_text(&kind.display_name())
            ));
            name_lbl.set_halign(gtk4::Align::Start);
            name_lbl.set_hexpand(true);
            top_line.append(&name_lbl);
            row_box.append(&top_line);

            let desc_lbl = gtk4::Label::new(None);
            desc_lbl.set_markup(&format!(
                "<small>{}</small>",
                glib::markup_escape_text(&kind.description())
            ));
            desc_lbl.set_halign(gtk4::Align::Start);
            desc_lbl.add_css_class("dim-label");
            row_box.append(&desc_lbl);

            // Mouse hover selects this item
            {
                let motion = gtk4::EventControllerMotion::new();
                let sel_weak1 = Rc::downgrade(self);
                motion.connect_enter(move |_, _, _| {
                    if let Some(sel) = sel_weak1.upgrade() {
                        sel.set_selected_index(idx);
                    }
                });
                let sel_weak2 = Rc::downgrade(self);
                motion.connect_motion(move |_, _, _| {
                    if let Some(sel) = sel_weak2.upgrade()
                        && sel.selected_index.get() != idx
                    {
                        sel.set_selected_index(idx);
                    }
                });
                row_box.add_controller(motion);
            }

            // Mouse click selects and confirms this item immediately
            {
                let click = gtk4::GestureClick::new();
                click.set_button(1);
                click.set_propagation_phase(gtk4::PropagationPhase::Capture);
                let sel_weak = Rc::downgrade(self);
                click.connect_pressed(move |gesture, _n_press, _x, _y| {
                    if let Some(sel) = sel_weak.upgrade() {
                        gesture.set_state(gtk4::EventSequenceState::Claimed);
                        sel.set_selected_index(idx);
                        sel.confirm_current();
                    }
                });
                row_box.add_controller(click);
            }

            self.items_box.append(&row_box);
            self.row_widgets.borrow_mut().push(row_box);
        }

        self.refresh_selection_visuals();
    }

    fn refresh_selection_visuals(&self) {
        let sel_idx = self.selected_index.get();
        for (idx, row) in self.row_widgets.borrow().iter().enumerate() {
            if idx == sel_idx {
                row.add_css_class("fwer-pane-selector-item-selected");
            } else {
                row.remove_css_class("fwer-pane-selector-item-selected");
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.pending_request.get().is_some() && self.backdrop.is_visible()
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index.get()
    }

    pub fn selected_kind(&self) -> PaneKind {
        self.items
            .get(self.selected_index.get())
            .copied()
            .unwrap_or(PaneKind::Shell)
    }

    pub fn set_selected_index(&self, idx: usize) {
        if self.items.is_empty() {
            return;
        }
        let clamped = idx.min(self.items.len() - 1);
        self.selected_index.set(clamped);
        self.refresh_selection_visuals();
    }

    pub fn open(self: &Rc<Self>, target_id: TileId, horizontal: bool) {
        self.pending_request.set(Some(PendingSplitRequest {
            target_id,
            horizontal,
        }));
        self.selected_index.set(0);
        self.rebuild_rows();

        let dir_label = if horizontal {
            format!("↔ {}", tr("Split Horizontally"))
        } else {
            format!("↕ {}", tr("Split Vertically"))
        };
        self.title_label.set_markup(&format!(
            "<b>{} — {}</b>",
            glib::markup_escape_text(&dir_label),
            glib::markup_escape_text(&tr("Select Panel Type"))
        ));
        self.hint_label.set_markup(&format!(
            "<small>{}</small>",
            glib::markup_escape_text(&tr(
                "J/K, ↑/↓: Select  |  Tab / Click: Confirm  |  Esc: Cancel"
            ))
        ));

        self.backdrop.set_visible(true);
        self.card.grab_focus();

        let card_clone = self.card.clone();
        glib::idle_add_local_once(move || {
            if card_clone.is_visible() {
                card_clone.grab_focus();
            }
        });
    }

    pub fn confirm_current(&self) {
        let Some(req) = self.pending_request.take() else {
            return;
        };
        let kind = self.selected_kind();
        self.backdrop.set_visible(false);
        (self.on_confirm)(req, kind);
    }

    pub fn cancel(&self) {
        if self.pending_request.take().is_some() {
            self.backdrop.set_visible(false);
            (self.on_cancel)();
        }
    }

    /// Processes a key press while the selector is open. Returns `true` if handled.
    pub fn handle_key_press(&self, key: gdk::Key) -> bool {
        if !self.is_open() {
            return false;
        }
        match map_selector_key(key) {
            Some(SelectorKeyAction::MoveNext) => {
                let next = next_selector_index(self.selected_index.get(), self.items.len(), 1);
                self.set_selected_index(next);
                true
            }
            Some(SelectorKeyAction::MovePrev) => {
                let prev = next_selector_index(self.selected_index.get(), self.items.len(), -1);
                self.set_selected_index(prev);
                true
            }
            Some(SelectorKeyAction::Confirm) => {
                self.confirm_current();
                true
            }
            Some(SelectorKeyAction::Cancel) => {
                self.cancel();
                true
            }
            None => true, // Block other keys from leaking into the terminal while selector is open
        }
    }
}
