// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::config::layout_preset::{
    LayoutNode, PanePresetConfig, PresetStore, WorkspacePreset, expand_tilde_path,
};
use crate::config::theme::{ThemeSettings, format_rgb_hex};
use crate::menus::MenuManager;
use crate::menus::renderer::show_context_menu_popover;
use crate::menus::types::{MenuAction, MenuContext, MenuTargetKind};
use crate::ui::empty_pane::{
    EMPTY_PANE_FRAME_INSET, compute_pass_through_hole_rect, set_x11_surface_keep_above,
};
use crate::ui::pane::{PaneEvent, PaneKind, PaneShortcutAction, PaneWidget, SpecialModeKey};
use crate::ui::pane_selector::PaneSelectorWidget;
use crate::ui::state::{InteractionMode, PaneContext, TileId};
use crate::ui::titlebar::{TitlebarEvent, next_mode_on_grip_click};
use crate::ui::tree_ops::{Direction, TreeNode, WorkspaceTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceWindowAction {
    MinimizeWindow,
    ToggleMaximizeWindow,
    OpenPreferences,
    CloseApplication,
}

pub struct Workspace {
    pub root_box: gtk4::Box,
    tree_box: gtk4::Box,
    pub pane_selector: Rc<PaneSelectorWidget>,
    pub tree: RefCell<WorkspaceTree>,
    pane_widgets: RefCell<HashMap<TileId, PaneWidget>>,
    mode: Cell<InteractionMode>,
    kb_target_pane: Cell<Option<TileId>>,
    opacity_selected_panes: RefCell<Vec<TileId>>,
    opacity_confirmed: Cell<bool>,
    modal_blocked: Cell<bool>,
    window_focused: Cell<bool>,
    settings: RefCell<ThemeSettings>,
    pub menu_manager: RefCell<MenuManager>,
    css_provider: gtk4::CssProvider,
    last_global_css: RefCell<String>,
    rebuilding: Cell<bool>,
    fps_control_enabled: Cell<bool>,
    pass_through_sync_pending: Cell<bool>,
    last_keep_above: Cell<bool>,
    on_window_action: RefCell<Option<Box<dyn Fn(WorkspaceWindowAction) + 'static>>>,
}

impl Workspace {
    pub fn new(
        initial_settings: ThemeSettings,
        startup_command: Option<String>,
        on_window_action: impl Fn(WorkspaceWindowAction) + 'static,
    ) -> Rc<Self> {
        let root_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root_box.set_hexpand(true);
        root_box.set_vexpand(true);
        root_box.add_css_class("fwer-workspace");
        root_box.add_css_class("fwer-window-frame");

        let workspace_overlay = gtk4::Overlay::new();
        workspace_overlay.set_hexpand(true);
        workspace_overlay.set_vexpand(true);

        let tree_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        tree_box.set_hexpand(true);
        tree_box.set_vexpand(true);
        workspace_overlay.set_child(Some(&tree_box));

        let css_provider = gtk4::CssProvider::new();
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &css_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let default_title = default_shell_title();
        let (tree, first_id) =
            WorkspaceTree::with_initial_pane(&default_title, startup_command.clone());

        let ws_holder: Rc<RefCell<Option<Rc<Self>>>> = Rc::new(RefCell::new(None));
        let holder_confirm = Rc::clone(&ws_holder);
        let holder_cancel = Rc::clone(&ws_holder);

        let pane_selector = PaneSelectorWidget::new(
            move |req, kind| {
                if let Some(ws) = holder_confirm.borrow().clone() {
                    ws.split_pane_with_kind(req.target_id, req.horizontal, kind);
                }
            },
            move || {
                if let Some(ws) = holder_cancel.borrow().clone() {
                    ws.refresh_all_panes();
                    ws.focus_active_pane();
                }
            },
        );
        workspace_overlay.add_overlay(&pane_selector.backdrop);
        root_box.append(&workspace_overlay);

        let ws = Rc::new(Self {
            root_box,
            tree_box,
            pane_selector,
            tree: RefCell::new(tree),
            pane_widgets: RefCell::new(HashMap::new()),
            mode: Cell::new(InteractionMode::Normal),
            kb_target_pane: Cell::new(None),
            opacity_selected_panes: RefCell::new(Vec::new()),
            opacity_confirmed: Cell::new(false),
            modal_blocked: Cell::new(false),
            window_focused: Cell::new(true),
            settings: RefCell::new(initial_settings),
            menu_manager: RefCell::new(MenuManager::new()),
            css_provider,
            last_global_css: RefCell::new(String::new()),
            rebuilding: Cell::new(false),
            fps_control_enabled: Cell::new(false),
            pass_through_sync_pending: Cell::new(false),
            last_keep_above: Cell::new(false),
            on_window_action: RefCell::new(Some(Box::new(on_window_action))),
        });
        *ws_holder.borrow_mut() = Some(Rc::clone(&ws));

        // Capture key events at workspace level when the pane selector modal is open
        {
            let key_ctrl = gtk4::EventControllerKey::new();
            key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let ws_weak = Rc::downgrade(&ws);
            key_ctrl.connect_key_pressed(move |_ctrl, key, _code, _mods| {
                let Some(ws) = ws_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if ws.pane_selector.is_open() && ws.pane_selector.handle_key_press(key) {
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            ws.root_box.add_controller(key_ctrl);
        }

        if startup_command.is_none()
            && let Some(startup_preset) = PresetStore::load().startup_preset().cloned()
        {
            ws.apply_preset(&startup_preset);
        } else {
            ws.update_global_css();
            ws.spawn_and_register_pane_with_kind(
                PaneKind::Shell,
                first_id,
                &default_title,
                None,
                startup_command.as_deref(),
            );
            ws.rebuild_gtk_tree();
            ws.refresh_all_panes();
            ws.focus_active_pane();
        }

        ws
    }

    pub fn set_window_focused(self: &Rc<Self>, focused: bool) {
        if self.window_focused.get() != focused {
            self.window_focused.set(focused);
            self.refresh_all_panes();
        }
    }

    pub fn is_window_focused(&self) -> bool {
        self.window_focused.get()
    }

    pub fn set_fps_control_enabled(&self, enabled: bool) {
        self.fps_control_enabled.set(enabled);
    }

    pub fn is_fps_control_enabled(&self) -> bool {
        self.fps_control_enabled.get()
    }

    pub fn set_modal_blocked(self: &Rc<Self>, blocked: bool) {
        self.modal_blocked.set(blocked);
        if blocked {
            if self.pane_selector.is_open() {
                self.pane_selector.cancel();
            }
            self.mode.set(InteractionMode::BlockedByModal);
            self.reset_opacity_mode_state();
        } else if self.mode.get() == InteractionMode::BlockedByModal {
            self.mode.set(InteractionMode::Normal);
        }
        self.refresh_all_panes();
    }

    fn reset_opacity_mode_state(&self) {
        self.kb_target_pane.set(None);
        self.opacity_selected_panes.borrow_mut().clear();
        self.opacity_confirmed.set(false);
    }

    pub fn apply_settings(self: &Rc<Self>, new_settings: ThemeSettings) {
        *self.settings.borrow_mut() = new_settings;
        self.update_global_css();
        self.refresh_all_panes();
    }

    pub fn settings(&self) -> ThemeSettings {
        self.settings.borrow().clone()
    }

    fn update_global_css(&self) {
        let s = self.settings.borrow();
        let sep_hex = format_rgb_hex(s.effective_separator_rgb());
        let sep_border_hex = format_rgb_hex(s.effective_separator_border_rgb());
        let sep_w = s.effective_separator_width().round() as i32;
        let frame_border = format_rgb_hex(s.effective_frame_border_rgb());
        let border_w = s.effective_border_width().round() as i32;
        let ui_font_pt = s.effective_ui_font_size();
        let font_family = s.effective_font_family();

        let css = format!(
            "window.fwer-main-window {{
                background-color: transparent;
             }}
             terminal, vte-terminal {{
                background-color: transparent;
             }}
             .fwer-window-frame {{
                background-color: transparent;
                border: {border_w}px solid {frame_border};
                padding: 0px;
                margin: 0px;
             }}
             .fwer-empty-pane,
             .fwer-empty-pane-hole {{
                background-color: transparent;
                background-image: none;
             }}
             .fwer-empty-pane-body {{
                background-color: transparent;
                background-image: none;
                border: {EMPTY_PANE_FRAME_INSET}px solid {frame_border};
             }}
             .fwer-pane-selector-backdrop {{
                background-color: rgba(0, 0, 0, 0.35);
             }}
             .fwer-pane-selector-card {{
                background-color: #1e1e24;
                color: #e6e6e6;
                border: 1px solid #5a5a64;
                border-radius: 8px;
                padding: 12px;
                font-family: \"{font_family}\", monospace;
                font-size: {ui_font_pt}pt;
             }}
             .fwer-pane-selector-title {{
                color: #f4f4f5;
                font-size: {ui_font_pt}pt;
             }}
             .fwer-pane-selector-hint {{
                color: #a1a1aa;
                margin-top: 4px;
             }}
             .fwer-pane-selector-item {{
                background-color: rgba(255, 255, 255, 0.03);
                border: 1px solid transparent;
                border-radius: 6px;
                padding: 8px 10px;
             }}
             .fwer-pane-selector-item-selected {{
                background-color: rgba(56, 189, 248, 0.20);
                border: 1px solid #38bdf8;
                border-radius: 6px;
                padding: 8px 10px;
             }}
             .fwer-workspace paned > separator,
             .fwer-workspace paned > separator.wide {{
                background-image: none;
                background-color: {sep_hex};
                border: none;
                box-shadow: none;
                outline: none;
                min-width: {sep_w}px;
                min-height: {sep_w}px;
             }}
             .fwer-workspace paned.horizontal > :first-child.fwer-pane,
             .fwer-workspace paned.horizontal > :first-child .fwer-pane {{
                border-right: 1px solid {sep_border_hex};
             }}
             .fwer-workspace paned.horizontal > :last-child.fwer-pane,
             .fwer-workspace paned.horizontal > :last-child .fwer-pane {{
                border-left: 1px solid {sep_border_hex};
             }}
             .fwer-workspace paned.vertical > :first-child.fwer-pane,
             .fwer-workspace paned.vertical > :first-child .fwer-pane {{
                border-bottom: 1px solid {sep_border_hex};
             }}
             .fwer-workspace paned.vertical > :last-child.fwer-pane,
             .fwer-workspace paned.vertical > :last-child .fwer-pane {{
                border-top: 1px solid {sep_border_hex};
             }}
             .fwer-disabled-row, .fwer-disabled-row * {{
                opacity: 0.35;
                color: #7a7a7a;
             }}
             .fwer-font-card {{
                background-color: #1e1e22;
                border: 1px solid #414148;
                border-radius: 4px;
                padding: 6px 8px;
                color: #e6e6e6;
             }}
             .fwer-font-card-selected {{
                background-color: #23372a;
                border: 2px solid #4ade80;
                border-radius: 4px;
                padding: 6px 8px;
                color: #e6e6e6;
             }}
             .fwer-preset-col {{
                background-color: rgba(26, 26, 30, 0.65);
                border: 1px solid #3f3f46;
                border-radius: 6px;
                padding: 8px;
             }}
             .fwer-preset-item {{
                background-color: transparent;
                border: 1px solid transparent;
                border-radius: 4px;
                padding: 5px 8px;
             }}
             .fwer-preset-item:hover {{
                background-color: rgba(255, 255, 255, 0.08);
             }}
             .fwer-preset-item-selected {{
                background-color: rgba(56, 189, 248, 0.18);
                border: 1px solid #38bdf8;
                border-radius: 4px;
                padding: 5px 8px;
             }}
             .fwer-preset-group-node {{
                color: #a1a1aa;
                padding: 3px 6px;
             }}
             .fwer-preset-inspector {{
                background-color: rgba(30, 30, 35, 0.75);
                border: 1px solid #45454f;
                border-radius: 6px;
                padding: 12px;
             }}
             .fwer-pane-moving-source {{
                border: 2px solid rgb(250, 204, 21);
             }}
             .fwer-pane-broadcast-source {{
                border: 2px solid rgb(56, 189, 248);
             }}
             .fwer-pane-resizing-source {{
                border: 2px solid rgb(74, 222, 128);
             }}
             .fwer-pane-opacity-source {{
                border: 2px solid rgb(236, 72, 153);
             }}
             .fwer-pane-target-hover {{
                border: 2px solid rgb(80, 180, 255);
             }}
             .fwer-scroll-badge {{
                background-color: rgba(20, 20, 20, 0.85);
                color: rgb(250, 204, 21);
                font-weight: bold;
                padding: 2px 6px;
                border-radius: 4px;
                font-family: \"{font_family}\", monospace;
                font-size: {ui_font_pt}pt;
             }}
             .fwer-context-popover contents {{
                background-color: #252526;
                color: #e6e6e6;
                border: 1px solid #5a5a5a;
                border-radius: 6px;
                font-family: \"{font_family}\", monospace;
                font-size: {ui_font_pt}pt;
             }}
             .fwer-menu-item {{
                color: #e6e6e6;
                padding: 4px 8px;
                border-radius: 4px;
             }}
             .fwer-menu-item:hover {{
                background-color: rgba(255, 255, 255, 0.12);
             }}
             .fwer-menu-shortcut {{
                color: #9da5b4;
                font-size: {:.1}pt;
                margin-left: 16px;
             }}
             .fwer-workspace paned > separator,
             .fwer-workspace paned > separator.wide,
             .fwer-workspace paned.horizontal > separator,
             .fwer-workspace paned.horizontal > separator.wide,
             .fwer-workspace paned.vertical > separator,
             .fwer-workspace paned.vertical > separator.wide {{
                background-image: none;
                background-color: {sep_hex};
                border: none;
                border-width: 0px;
                box-shadow: none;
                outline: none;
                margin: 0px;
                min-width: {sep_w}px;
                min-height: {sep_w}px;
             }}",
            (ui_font_pt - 1.0).max(8.0)
        );
        if *self.last_global_css.borrow() != css {
            *self.last_global_css.borrow_mut() = css.clone();
            self.css_provider.load_from_string(&css);
        }
    }

    pub fn schedule_pass_through_sync(self: &Rc<Self>) {
        self.sync_empty_pane_pass_through();
        if self.pass_through_sync_pending.get() {
            return;
        }
        self.pass_through_sync_pending.set(true);
        let ws_weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(16), move || {
            if let Some(ws) = ws_weak.upgrade() {
                ws.pass_through_sync_pending.set(false);
                ws.sync_empty_pane_pass_through();
            }
        });
    }

    pub fn sync_empty_pane_pass_through(&self) {
        let Some(native) = self.root_box.native() else {
            return;
        };
        let Some(surface) = native.surface() else {
            return;
        };
        let surf_w = surface.width();
        let surf_h = surface.height();
        if surf_w <= 0 || surf_h <= 0 {
            return;
        }

        let visible_ids = self.tree.borrow().visible_pane_ids();
        let widgets = self.pane_widgets.borrow();

        let has_visible_empty = visible_ids
            .iter()
            .any(|id| widgets.get(id).is_some_and(|w| w.kind() == PaneKind::Empty));

        if self.last_keep_above.get() != has_visible_empty {
            self.last_keep_above.set(has_visible_empty);
            set_x11_surface_keep_above(&surface, has_visible_empty);
        }

        let full_rect = gtk4::cairo::RectangleInt::new(0, 0, surf_w, surf_h);
        let region = gtk4::cairo::Region::create_rectangle(&full_rect);

        let allow_holes = has_visible_empty
            && self.mode.get() == InteractionMode::Normal
            && !self.modal_blocked.get()
            && !self.pane_selector.is_open();

        if allow_holes {
            for id in visible_ids {
                if let Some(pane) = widgets.get(&id)
                    && let Some(hole_widget) = pane.pass_through_widget()
                    && hole_widget.is_mapped()
                    && let Some(bounds) = hole_widget.compute_bounds(&native)
                    && let Some((hx, hy, hw, hh)) = compute_pass_through_hole_rect(
                    bounds.x(),
                    bounds.y(),
                    bounds.width(),
                    bounds.height(),
                    surf_w,
                    surf_h,
                    EMPTY_PANE_FRAME_INSET,
                )
                {
                    let hole_rect = gtk4::cairo::RectangleInt::new(hx, hy, hw, hh);
                    let _ = region.subtract_rectangle(&hole_rect);
                }
            }
        }

        surface.set_input_region(Some(&region));
    }

    fn spawn_and_register_pane_with_kind(
        self: &Rc<Self>,
        kind: PaneKind,
        tile_id: TileId,
        title: &str,
        working_dir: Option<&Path>,
        startup_command: Option<&str>,
    ) -> PaneWidget {
        let ws_weak = Rc::downgrade(self);
        let settings = self.settings.borrow().clone();
        let pane = PaneWidget::new_for_kind(
            kind,
            tile_id,
            title,
            working_dir,
            startup_command,
            &settings,
            move |ev| {
                if let Some(ws) = ws_weak.upgrade() {
                    ws.handle_pane_event(ev);
                }
            },
        );
        self.pane_widgets.borrow_mut().insert(tile_id, pane.clone());
        pane
    }

    fn handle_pane_event(self: &Rc<Self>, event: PaneEvent) {
        if self.pane_selector.is_open() {
            return;
        }
        match event {
            PaneEvent::Titlebar(tb_ev) => self.handle_titlebar_event(tb_ev),
            PaneEvent::ClickedInSpecialMode {
                tile_id,
                keep_selecting,
            } => {
                let cur_mode = self.mode.get();
                match cur_mode {
                    InteractionMode::MovingPane(src_id) => {
                        self.mode.set(InteractionMode::Normal);
                        self.kb_target_pane.set(None);
                        if src_id != tile_id {
                            let swapped = self.tree.borrow_mut().swap_panes(src_id, tile_id);
                            if swapped {
                                self.tree.borrow_mut().set_active_pane(src_id);
                                self.rebuild_gtk_tree();
                            }
                        }
                        self.refresh_all_panes();
                        self.focus_active_pane();
                    }

                    InteractionMode::SelectingBroadcast(src_id) => {
                        if src_id == tile_id {
                            self.tree.borrow_mut().leave_broadcast_group(src_id);
                            self.mode.set(InteractionMode::Normal);
                            self.kb_target_pane.set(None);
                        } else {
                            self.tree
                                .borrow_mut()
                                .link_broadcast_panes(src_id, tile_id);
                            if !keep_selecting {
                                self.mode.set(InteractionMode::Normal);
                                self.kb_target_pane.set(None);
                            }
                        }
                        self.refresh_all_panes();
                        self.focus_active_pane();
                    }

                    InteractionMode::AdjustingOpacity(_) => {
                        if self.opacity_confirmed.get() && !keep_selecting {
                            self.mode.set(InteractionMode::Normal);
                            self.reset_opacity_mode_state();
                            let _ = self.settings().save();
                        } else {
                            self.select_opacity_pane(tile_id, keep_selecting);
                        }
                        self.refresh_all_panes();
                        self.focus_active_pane();
                    }
                    _ => {}
                }
            }
            PaneEvent::OpacityScroll { tile_id: _, delta } => {
                if matches!(self.mode.get(), InteractionMode::AdjustingOpacity(_)) {
                    self.apply_opacity_delta(delta);
                    self.focus_active_pane();
                }
            }
            PaneEvent::OpenPaneContextMenu { tile_id, x, y } => {
                self.open_context_menu_for(MenuTargetKind::Pane, tile_id, x, y);
            }
            PaneEvent::ShortcutTriggered { tile_id, action } => {
                self.execute_shortcut_action(tile_id, action);
            }
            PaneEvent::SpecialModeKeyInput { tile_id, key } => {
                self.handle_special_mode_key(tile_id, key);
            }
            PaneEvent::CancelSpecialMode => {
                let was_opacity = matches!(self.mode.get(), InteractionMode::AdjustingOpacity(_));
                self.mode.set(InteractionMode::Normal);
                self.reset_opacity_mode_state();
                if was_opacity {
                    let _ = self.settings().save();
                }
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            PaneEvent::BroadcastInput { source_id, bytes } => {
                let peers = self.tree.borrow().broadcast_peers(source_id);
                let widgets = self.pane_widgets.borrow();
                for peer_id in peers {
                    if let Some(peer_pane) = widgets.get(&peer_id) {
                        peer_pane.write_input(&bytes);
                    }
                }
            }
            PaneEvent::ChildExited(tile_id) => {
                let ws_clone = Rc::clone(self);
                glib::idle_add_local_once(move || {
                    ws_clone.close_pane_by_id(tile_id);
                });
            }
        }
    }

    fn select_opacity_pane(&self, tile_id: TileId, keep_selecting: bool) {
        self.kb_target_pane.set(Some(tile_id));
        let mut selected = self.opacity_selected_panes.borrow_mut();
        if keep_selecting {
            if let Some(pos) = selected.iter().position(|&id| id == tile_id) {
                selected.remove(pos);
            } else {
                selected.push(tile_id);
            }
        } else {
            if !selected.contains(&tile_id) {
                selected.push(tile_id);
            }
            self.opacity_confirmed.set(true);
        }
    }

    fn apply_opacity_delta(self: &Rc<Self>, delta: f32) {
        let selected = self.opacity_selected_panes.borrow().clone();
        let kb_target = self.kb_target_pane.get();
        let confirmed = self.opacity_confirmed.get();

        if !confirmed && selected.is_empty() && kb_target.is_none() {
            let mut current_settings = self.settings.borrow().clone();
            current_settings.terminal_bg_opacity =
                (current_settings.terminal_bg_opacity + delta).clamp(0.0, 1.0);
            self.apply_settings(current_settings);
            return;
        }

        let mut targets = selected;
        if !confirmed
            && let Some(kb_id) = kb_target
            && !targets.contains(&kb_id)
        {
            targets.push(kb_id);
        }

        let settings = self.settings.borrow().clone();
        {
            let widgets = self.pane_widgets.borrow();
            for target_id in targets {
                if let Some(pane) = widgets.get(&target_id) {
                    pane.adjust_custom_opacity_by(delta, &settings);
                }
            }
        }
        self.refresh_all_panes();
    }

    fn handle_special_mode_key(self: &Rc<Self>, tile_id: TileId, key: SpecialModeKey) {
        match self.mode.get() {
            InteractionMode::AdjustingOpacity(src_id) => match key {
                SpecialModeKey::AdjustOpacity(sign) => {
                    self.apply_opacity_delta(sign as f32 * 0.05);
                    self.focus_active_pane();
                }
                SpecialModeKey::Move(dir) => {
                    let single_pane = self.pane_widgets.borrow().len() <= 1;
                    let is_initial = !self.opacity_confirmed.get()
                        && self.opacity_selected_panes.borrow().is_empty()
                        && self.kb_target_pane.get().is_none();

                    if self.opacity_confirmed.get() || (is_initial && single_pane) {
                        let delta = match dir {
                            Direction::Up | Direction::Right => 0.05,
                            Direction::Down | Direction::Left => -0.05,
                        };
                        self.apply_opacity_delta(delta);
                        self.focus_active_pane();
                    } else {
                        let cur_target = self.kb_target_pane.get().unwrap_or(src_id);
                        if let Some(next_target) = self.find_neighbor_pane(cur_target, dir) {
                            self.kb_target_pane.set(Some(next_target));
                        } else if self.kb_target_pane.get().is_none() {
                            self.kb_target_pane.set(Some(src_id));
                        }
                        self.refresh_all_panes();
                        self.focus_active_pane();
                    }
                }
                SpecialModeKey::KeepSelecting => {
                    let target = self.kb_target_pane.get().unwrap_or(src_id);
                    self.select_opacity_pane(target, true);
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
                SpecialModeKey::Confirm => {
                    if self.opacity_confirmed.get() {
                        self.mode.set(InteractionMode::Normal);
                        self.reset_opacity_mode_state();
                        let _ = self.settings().save();
                    } else {
                        let target = self.kb_target_pane.get().unwrap_or(src_id);
                        self.select_opacity_pane(target, false);
                    }
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
                SpecialModeKey::Cancel => {
                    self.mode.set(InteractionMode::Normal);
                    self.reset_opacity_mode_state();
                    let _ = self.settings().save();
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
            },
            InteractionMode::ResizingPane(src_id) => match key {
                SpecialModeKey::Move(dir) => {
                    if self
                        .tree
                        .borrow_mut()
                        .adjust_split_for_pane(src_id, dir, 0.05)
                    {
                        self.rebuild_gtk_tree();
                        self.refresh_all_panes();
                        self.focus_active_pane();
                    }
                }
                SpecialModeKey::AdjustOpacity(_) => {}
                SpecialModeKey::Confirm
                | SpecialModeKey::KeepSelecting
                | SpecialModeKey::Cancel => {
                    self.mode.set(InteractionMode::Normal);
                    self.kb_target_pane.set(None);
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
            },
            InteractionMode::MovingPane(src_id) => match key {
                SpecialModeKey::Move(dir) => {
                    if let Some(target_id) = self.find_neighbor_pane(src_id, dir) {
                        let swapped = self.tree.borrow_mut().swap_panes(src_id, target_id);
                        if swapped {
                            self.tree.borrow_mut().set_active_pane(src_id);
                            self.rebuild_gtk_tree();
                            self.refresh_all_panes();
                            self.focus_active_pane();
                        }
                    }
                }
                SpecialModeKey::AdjustOpacity(_) => {}
                SpecialModeKey::Confirm
                | SpecialModeKey::KeepSelecting
                | SpecialModeKey::Cancel => {
                    self.mode.set(InteractionMode::Normal);
                    self.kb_target_pane.set(None);
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
            },
            InteractionMode::SelectingBroadcast(src_id) => match key {
                SpecialModeKey::Move(dir) => {
                    let cur_target = self.kb_target_pane.get().unwrap_or(src_id);
                    if let Some(next_target) = self.find_neighbor_pane(cur_target, dir) {
                        self.kb_target_pane.set(Some(next_target));
                        self.refresh_all_panes();
                    }
                }
                SpecialModeKey::AdjustOpacity(_) => {}
                SpecialModeKey::KeepSelecting => {
                    let target = self.kb_target_pane.get().unwrap_or(tile_id);
                    self.toggle_or_link_broadcast(src_id, target);
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
                SpecialModeKey::Confirm => {
                    let target = self.kb_target_pane.get().unwrap_or(tile_id);
                    self.toggle_or_link_broadcast(src_id, target);
                    self.mode.set(InteractionMode::Normal);
                    self.kb_target_pane.set(None);
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
                SpecialModeKey::Cancel => {
                    self.mode.set(InteractionMode::Normal);
                    self.kb_target_pane.set(None);
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
            },
            _ => {}
        }
    }

    fn toggle_or_link_broadcast(&self, src_id: TileId, target_id: TileId) {
        let mut tree = self.tree.borrow_mut();
        if src_id == target_id {
            tree.leave_broadcast_group(src_id);
        } else {
            let src_grp = tree
                .panes
                .get(&src_id)
                .map(|p| p.broadcast_group)
                .unwrap_or(0);
            let dst_grp = tree
                .panes
                .get(&target_id)
                .map(|p| p.broadcast_group)
                .unwrap_or(0);
            if src_grp > 0 && src_grp == dst_grp {
                tree.leave_broadcast_group(target_id);
            } else {
                tree.link_broadcast_panes(src_id, target_id);
            }
        }
    }

    fn find_neighbor_pane(&self, from_id: TileId, dir: Direction) -> Option<TileId> {
        let widgets = self.pane_widgets.borrow();
        let from_pane = widgets.get(&from_id)?;
        let from_rect = from_pane.root().compute_bounds(&self.root_box)?;
        let fx = (from_rect.x() + from_rect.width() * 0.5) as f64;
        let fy = (from_rect.y() + from_rect.height() * 0.5) as f64;

        let mut best: Option<(TileId, f64)> = None;

        for (&id, pane) in widgets.iter() {
            if id == from_id || !pane.root().is_mapped() {
                continue;
            }
            let Some(r) = pane.root().compute_bounds(&self.root_box) else {
                continue;
            };
            let cx = (r.x() + r.width() * 0.5) as f64;
            let cy = (r.y() + r.height() * 0.5) as f64;
            let dx = cx - fx;
            let dy = cy - fy;

            let valid = match dir {
                Direction::Left => dx < -5.0,
                Direction::Right => dx > 5.0,
                Direction::Up => dy < -5.0,
                Direction::Down => dy > 5.0,
            };
            if !valid {
                continue;
            }

            let score = match dir {
                Direction::Left | Direction::Right => dx.abs() + dy.abs() * 2.0,
                Direction::Up | Direction::Down => dy.abs() + dx.abs() * 2.0,
            };

            if best.is_none_or(|(_, b_score)| score < b_score) {
                best = Some((id, score));
            }
        }

        best.map(|(id, _)| id)
    }

    fn commit_any_active_title_edit(&self) {
        let mut updates = Vec::new();
        for (&id, pane) in self.pane_widgets.borrow().iter() {
            if let Some(new_title) = pane.finish_title_edit_if_active() {
                updates.push((id, new_title));
            }
        }
        if !updates.is_empty() {
            let mut tree = self.tree.borrow_mut();
            for (id, new_title) in updates {
                if let Some(meta) = tree.panes.get_mut(&id)
                    && !new_title.trim().is_empty()
                {
                    meta.title = new_title.trim().to_string();
                }
            }
        }
        if matches!(self.mode.get(), InteractionMode::EditingTitle(_)) {
            self.mode.set(InteractionMode::Normal);
        }
    }

    fn handle_titlebar_event(self: &Rc<Self>, event: TitlebarEvent) {
        if self.modal_blocked.get() || self.pane_selector.is_open() {
            return;
        }
        match event {
            TitlebarEvent::FocusPane(tile_id) => {
                if matches!(
                    self.mode.get(),
                    InteractionMode::MovingPane(_)
                        | InteractionMode::SelectingBroadcast(_)
                        | InteractionMode::AdjustingOpacity(_)
                ) {
                    self.handle_pane_event(PaneEvent::ClickedInSpecialMode {
                        tile_id,
                        keep_selecting: false,
                    });
                    return;
                }
                if matches!(self.mode.get(), InteractionMode::ResizingPane(_)) {
                    self.mode.set(InteractionMode::Normal);
                }
                self.commit_any_active_title_edit();
                self.window_focused.set(true);
                self.tree.borrow_mut().set_active_pane(tile_id);
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            TitlebarEvent::ToggleMoveMode(tile_id) => {
                if matches!(self.mode.get(), InteractionMode::AdjustingOpacity(_)) {
                    if let Some(pane) = self.pane_widgets.borrow().get(&tile_id) {
                        pane.set_custom_opacity(None);
                    }
                    self.refresh_all_panes();
                    self.focus_active_pane();
                    return;
                }
                self.commit_any_active_title_edit();
                self.tree.borrow_mut().set_active_pane(tile_id);
                let next = next_mode_on_grip_click(self.mode.get(), tile_id, false);
                self.mode.set(next);
                self.reset_opacity_mode_state();
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            TitlebarEvent::ToggleBroadcastSelectMode(tile_id) => {
                self.commit_any_active_title_edit();
                self.tree.borrow_mut().set_active_pane(tile_id);
                let next = next_mode_on_grip_click(self.mode.get(), tile_id, true);
                self.mode.set(next);
                self.reset_opacity_mode_state();
                self.kb_target_pane.set(Some(tile_id));
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            TitlebarEvent::StartEditingTitle(tile_id) => {
                for (&id, pane) in self.pane_widgets.borrow().iter() {
                    if id != tile_id
                        && let Some(new_title) = pane.finish_title_edit_if_active()
                        && let Some(meta) = self.tree.borrow_mut().panes.get_mut(&id)
                        && !new_title.trim().is_empty()
                    {
                        meta.title = new_title.trim().to_string();
                    }
                }
                self.mode.set(InteractionMode::EditingTitle(tile_id));
                self.refresh_all_panes();
            }
            TitlebarEvent::FinishEditingTitle(tile_id, new_title) => {
                if let Some(meta) = self.tree.borrow_mut().panes.get_mut(&tile_id)
                    && !new_title.trim().is_empty()
                {
                    meta.title = new_title.trim().to_string();
                }
                if self.mode.get() == InteractionMode::EditingTitle(tile_id) {
                    self.mode.set(InteractionMode::Normal);
                }
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            TitlebarEvent::CancelEditingTitle(tile_id) => {
                if self.mode.get() == InteractionMode::EditingTitle(tile_id) {
                    self.mode.set(InteractionMode::Normal);
                }
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            TitlebarEvent::OpenPreferences => {
                self.emit_window_action(WorkspaceWindowAction::OpenPreferences);
            }
            TitlebarEvent::MinimizeWindow => {
                self.emit_window_action(WorkspaceWindowAction::MinimizeWindow);
            }
            TitlebarEvent::ToggleMaximizeWindow => {
                self.emit_window_action(WorkspaceWindowAction::ToggleMaximizeWindow);
            }
            TitlebarEvent::ClosePane(tile_id) => {
                self.close_pane_by_id(tile_id);
            }
            TitlebarEvent::OpenContextMenu {
                tile_id,
                x_bits,
                y_bits,
            } => {
                let x = f64::from_bits(x_bits);
                let y = f64::from_bits(y_bits);
                self.open_context_menu_for(MenuTargetKind::Titlebar, tile_id, x, y);
            }
        }
    }

    pub fn execute_shortcut_action(self: &Rc<Self>, tile_id: TileId, action: PaneShortcutAction) {
        match action {
            PaneShortcutAction::Copy => {
                if let Some(pane) = self.pane_widgets.borrow().get(&tile_id) {
                    pane.copy_selection_or_screen();
                }
            }
            PaneShortcutAction::Paste => {
                if let Some(pane) = self.pane_widgets.borrow().get(&tile_id) {
                    pane.paste_clipboard();
                }
            }
            PaneShortcutAction::SplitHorizontal => {
                self.split_pane(tile_id, true);
            }
            PaneShortcutAction::SplitVertical => {
                self.split_pane(tile_id, false);
            }
            PaneShortcutAction::NewTab => {
                self.add_tab(tile_id);
            }
            PaneShortcutAction::ClosePane => {
                self.close_pane_by_id(tile_id);
            }
            PaneShortcutAction::FocusNextPane => {
                if self.tree.borrow_mut().focus_next_pane().is_some() {
                    self.rebuild_gtk_tree();
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
            }
            PaneShortcutAction::FocusPrevPane => {
                if self.tree.borrow_mut().focus_prev_pane().is_some() {
                    self.rebuild_gtk_tree();
                    self.refresh_all_panes();
                    self.focus_active_pane();
                }
            }
            PaneShortcutAction::OpenPreferences => {
                self.emit_window_action(WorkspaceWindowAction::OpenPreferences);
            }
            PaneShortcutAction::ToggleFullscreen => {
                self.emit_window_action(WorkspaceWindowAction::ToggleMaximizeWindow);
            }
            PaneShortcutAction::ZoomIn => {
                if let Some(pane) = self.pane_widgets.borrow().get(&tile_id) {
                    pane.adjust_zoom_by(0.1);
                }
            }
            PaneShortcutAction::ZoomOut => {
                if let Some(pane) = self.pane_widgets.borrow().get(&tile_id) {
                    pane.adjust_zoom_by(-0.1);
                }
            }
            PaneShortcutAction::ZoomReset => {
                if let Some(pane) = self.pane_widgets.borrow().get(&tile_id) {
                    pane.reset_zoom();
                }
            }
            PaneShortcutAction::ToggleMoveMode => {
                self.commit_any_active_title_edit();
                let next = if self.mode.get() == InteractionMode::MovingPane(tile_id) {
                    InteractionMode::Normal
                } else {
                    InteractionMode::MovingPane(tile_id)
                };
                self.mode.set(next);
                self.reset_opacity_mode_state();
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            PaneShortcutAction::ToggleResizeMode => {
                self.commit_any_active_title_edit();
                let next = if self.mode.get() == InteractionMode::ResizingPane(tile_id) {
                    InteractionMode::Normal
                } else {
                    InteractionMode::ResizingPane(tile_id)
                };
                self.mode.set(next);
                self.reset_opacity_mode_state();
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            PaneShortcutAction::ToggleBroadcastMode => {
                self.commit_any_active_title_edit();
                let next = if self.mode.get() == InteractionMode::SelectingBroadcast(tile_id) {
                    InteractionMode::Normal
                } else {
                    InteractionMode::SelectingBroadcast(tile_id)
                };
                self.mode.set(next);
                self.reset_opacity_mode_state();
                self.kb_target_pane.set(Some(tile_id));
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            PaneShortcutAction::ToggleOpacityMode => {
                self.commit_any_active_title_edit();
                let was_opacity = matches!(self.mode.get(), InteractionMode::AdjustingOpacity(_));
                let next = if was_opacity {
                    InteractionMode::Normal
                } else {
                    InteractionMode::AdjustingOpacity(tile_id)
                };
                self.mode.set(next);
                self.reset_opacity_mode_state();
                if was_opacity {
                    let _ = self.settings().save();
                }
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            PaneShortcutAction::ToggleScrollbar => {
                if let Some(pane) = self.pane_widgets.borrow().get(&tile_id) {
                    pane.toggle_scrollbar();
                }
            }
        }
    }

    pub fn split_pane(self: &Rc<Self>, target_id: TileId, horizontal: bool) {
        if !self.pane_widgets.borrow().contains_key(&target_id) {
            return;
        }
        if self.settings.borrow().select_pane_type_on_split {
            self.commit_any_active_title_edit();
            self.pane_selector.open(target_id, horizontal);
            self.refresh_all_panes();
            return;
        }
        self.split_pane_with_kind(target_id, horizontal, PaneKind::Shell);
    }

    pub fn split_pane_with_kind(
        self: &Rc<Self>,
        target_id: TileId,
        horizontal: bool,
        kind: PaneKind,
    ) {
        let inherited_cwd = self
            .pane_widgets
            .borrow()
            .get(&target_id)
            .and_then(|p| p.current_working_dir());
        let title = kind.default_title();
        let new_id_opt = self
            .tree
            .borrow_mut()
            .split_pane_with_kind(target_id, horizontal, &title, None, kind);

        if let Some(new_id) = new_id_opt {
            self.spawn_and_register_pane_with_kind(
                kind,
                new_id,
                &title,
                inherited_cwd.as_deref(),
                None,
            );
            self.rebuild_gtk_tree();
            self.refresh_all_panes();
            self.focus_active_pane();
        }
    }

    pub fn add_tab(self: &Rc<Self>, target_id: TileId) {
        let inherited_cwd = self
            .pane_widgets
            .borrow()
            .get(&target_id)
            .and_then(|p| p.current_working_dir());
        let title = default_shell_title();
        let new_id_opt = self
            .tree
            .borrow_mut()
            .add_tab_with_kind(target_id, &title, None, PaneKind::Shell);

        if let Some(new_id) = new_id_opt {
            self.spawn_and_register_pane_with_kind(
                PaneKind::Shell,
                new_id,
                &title,
                inherited_cwd.as_deref(),
                None,
            );
            self.rebuild_gtk_tree();
            self.refresh_all_panes();
            self.focus_active_pane();
        }
    }

    pub fn close_pane_by_id(self: &Rc<Self>, target_id: TileId) {
        if !self.pane_widgets.borrow().contains_key(&target_id) {
            return;
        }
        let is_empty = self.tree.borrow_mut().close_pane(target_id);
        if let Some(removed) = self.pane_widgets.borrow_mut().remove(&target_id) {
            removed.detach_callbacks();
            if let Some(parent) = removed.root().parent()
                && let Ok(parent_box) = parent.downcast::<gtk4::Box>()
            {
                parent_box.remove(removed.root());
            }
        }
        if is_empty {
            self.emit_window_action(WorkspaceWindowAction::CloseApplication);
            return;
        }
        self.mode.set(InteractionMode::Normal);
        self.reset_opacity_mode_state();
        self.rebuild_gtk_tree();
        self.refresh_all_panes();
        self.focus_active_pane();
    }

    pub fn save_current_layout(&self) -> Result<std::path::PathBuf, String> {
        let tree = self.tree.borrow();
        let panes = self.pane_widgets.borrow();
        let node = LayoutNode::from_tree_with_configs(&tree, |id| {
            let meta = tree.panes.get(&id);
            let widget = panes.get(&id);
            let pane_kind = widget
                .map(|w| w.kind())
                .or_else(|| meta.map(|m| m.kind))
                .unwrap_or(PaneKind::Shell);
            let title = meta
                .map(|m| m.title.clone())
                .unwrap_or_else(|| pane_kind.default_title());
            let command = widget
                .and_then(|w| w.startup_command())
                .or_else(|| meta.and_then(|m| m.command.clone()))
                .filter(|s| !s.trim().is_empty());
            let working_dir = widget
                .and_then(|w| w.current_working_dir())
                .map(|p| p.to_string_lossy().to_string())
                .filter(|s| !s.trim().is_empty());
            let opacity = widget.and_then(|w| w.custom_opacity());
            let font_size = widget.and_then(|w| w.custom_font_size());
            let show_scrollbar = widget.and_then(|w| w.custom_show_scrollbar());
            PanePresetConfig {
                title,
                pane_kind,
                command,
                working_dir,
                opacity,
                font_size,
                show_scrollbar,
            }
        })
            .ok_or_else(|| "Empty workspace tree".to_string())?;
        node.save_to_disk()
    }

    pub fn load_saved_layout(self: &Rc<Self>) -> Result<(), String> {
        let node = LayoutNode::load_from_disk()?;
        let next_id = self.tree.borrow().next_id_counter();
        let (new_tree, pane_configs) = node.build_tree_with_configs_starting_at(next_id);

        for pane in self.pane_widgets.borrow().values() {
            pane.detach_callbacks();
            if let Some(parent) = pane.root().parent()
                && let Ok(parent_box) = parent.downcast::<gtk4::Box>()
            {
                parent_box.remove(pane.root());
            }
        }
        self.pane_widgets.borrow_mut().clear();

        let ordered_ids = new_tree.ordered_pane_ids();
        *self.tree.borrow_mut() = new_tree;
        self.mode.set(InteractionMode::Normal);
        self.reset_opacity_mode_state();

        for id in ordered_ids {
            let cfg = pane_configs.get(&id).cloned().unwrap_or_default();
            let wd_path = cfg
                .working_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .and_then(expand_tilde_path);
            let pane = self.spawn_and_register_pane_with_kind(
                cfg.pane_kind,
                id,
                &cfg.title,
                wd_path.as_deref(),
                None,
            );
            pane.set_custom_opacity(cfg.opacity);
            pane.set_custom_font_size(cfg.font_size);
            pane.set_custom_show_scrollbar(cfg.show_scrollbar);

            if cfg.pane_kind == PaneKind::Shell
                && let Some(cmd) = cfg
                .command
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                pane.set_startup_command(Some(cmd.clone()));
                let pane_for_cmd = pane.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(250),
                    move || {
                        if pane_for_cmd.is_alive() {
                            let line = format!("{}\n", cmd);
                            pane_for_cmd.write_input(line.as_bytes());
                        }
                    },
                );
            }
        }

        self.rebuild_gtk_tree();
        self.refresh_all_panes();
        self.focus_active_pane();
        Ok(())
    }

    pub fn capture_current_preset(&self, name: &str) -> WorkspacePreset {
        let tree = self.tree.borrow();
        let panes = self.pane_widgets.borrow();
        let root = LayoutNode::from_tree_with_configs(&tree, |id| {
            let meta = tree.panes.get(&id);
            let widget = panes.get(&id);
            let pane_kind = widget
                .map(|w| w.kind())
                .or_else(|| meta.map(|m| m.kind))
                .unwrap_or(PaneKind::Shell);
            let title = meta
                .map(|m| m.title.clone())
                .unwrap_or_else(|| pane_kind.default_title());
            let command = widget
                .and_then(|w| w.startup_command())
                .or_else(|| meta.and_then(|m| m.command.clone()))
                .filter(|s| !s.trim().is_empty());
            let working_dir = widget
                .and_then(|w| w.current_working_dir())
                .map(|p| p.to_string_lossy().to_string())
                .filter(|s| !s.trim().is_empty());
            let opacity = widget.and_then(|w| w.custom_opacity());
            let font_size = widget.and_then(|w| w.custom_font_size());
            let show_scrollbar = widget.and_then(|w| w.custom_show_scrollbar());
            PanePresetConfig {
                title,
                pane_kind,
                command,
                working_dir,
                opacity,
                font_size,
                show_scrollbar,
            }
        })
            .unwrap_or_else(|| LayoutNode::Pane {
                title: "Shell".to_string(),
                pane_kind: PaneKind::Shell,
                command: None,
                working_dir: None,
                opacity: None,
                font_size: None,
                show_scrollbar: None,
            });

        let s = self.settings.borrow();
        WorkspacePreset {
            name: name.to_string(),
            run_on_startup: false,
            window_opacity: Some(s.effective_terminal_bg_opacity()),
            window_font_size: Some(s.effective_terminal_font_size()),
            ui_font_size: Some(s.effective_ui_font_size()),
            root,
        }
    }

    pub fn apply_preset(self: &Rc<Self>, preset: &WorkspacePreset) {
        {
            let mut s = self.settings.borrow_mut();
            if let Some(op) = preset.window_opacity {
                s.terminal_bg_opacity = op.clamp(0.0, 1.0);
            }
            if let Some(fs) = preset.window_font_size {
                s.terminal_font_size = ((fs.clamp(8.0, 36.0) * 2.0).round()) / 2.0;
            }
            if let Some(uifs) = preset.ui_font_size {
                s.ui_font_size = ((uifs.clamp(9.0, 28.0) * 2.0).round()) / 2.0;
            }
        }
        self.update_global_css();

        let next_id = self.tree.borrow().next_id_counter();
        let (new_tree, pane_configs) = preset.root.build_tree_with_configs_starting_at(next_id);

        for pane in self.pane_widgets.borrow().values() {
            pane.detach_callbacks();
            if let Some(parent) = pane.root().parent()
                && let Ok(parent_box) = parent.downcast::<gtk4::Box>()
            {
                parent_box.remove(pane.root());
            }
        }
        self.pane_widgets.borrow_mut().clear();

        let ordered_ids = new_tree.ordered_pane_ids();
        *self.tree.borrow_mut() = new_tree;
        self.mode.set(InteractionMode::Normal);
        self.reset_opacity_mode_state();

        for id in ordered_ids {
            let cfg = pane_configs.get(&id).cloned().unwrap_or_default();
            let wd_str = cfg.working_dir.as_deref().filter(|s| !s.trim().is_empty());
            let wd_path = wd_str.and_then(expand_tilde_path);

            let pane = self.spawn_and_register_pane_with_kind(
                cfg.pane_kind,
                id,
                &cfg.title,
                wd_path.as_deref(),
                None,
            );
            pane.set_custom_opacity(cfg.opacity);
            pane.set_custom_font_size(cfg.font_size);
            pane.set_custom_show_scrollbar(cfg.show_scrollbar);

            if cfg.pane_kind == PaneKind::Shell
                && let Some(cmd) = cfg
                .command
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                pane.set_startup_command(Some(cmd.clone()));
                let pane_for_cmd = pane.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(250),
                    move || {
                        if pane_for_cmd.is_alive() {
                            let line = format!("{}\n", cmd);
                            pane_for_cmd.write_input(line.as_bytes());
                        }
                    },
                );
            }
        }

        self.rebuild_gtk_tree();
        self.refresh_all_panes();
        self.focus_active_pane();
    }

    fn open_context_menu_for(
        self: &Rc<Self>,
        target_kind: MenuTargetKind,
        tile_id: TileId,
        x: f64,
        y: f64,
    ) {
        let Some(pane) = self.pane_widgets.borrow().get(&tile_id).cloned() else {
            return;
        };
        let (show_scrollbar, shortcuts) = {
            let s = self.settings.borrow();
            (pane.effective_show_scrollbar(&s), s.shortcuts.clone())
        };
        let ctx = MenuContext {
            target_kind,
            tile_id,
            has_selection: pane.has_selection(),
            show_scrollbar,
            is_read_only: pane.is_read_only(),
            shortcuts,
        };
        let preset_names: Vec<String> = PresetStore::load()
            .presets
            .into_iter()
            .map(|p| p.name)
            .collect();
        let entries = self
            .menu_manager
            .borrow()
            .build_entries_with_presets(&ctx, &preset_names);

        let anchor_widget: gtk4::Widget = match target_kind {
            MenuTargetKind::Pane => pane.root().clone().upcast(),
            MenuTargetKind::Titlebar => pane.titlebar().root.clone().upcast(),
        };

        let (root_x, root_y) = anchor_widget
            .compute_point(
                &self.root_box,
                &gtk4::graphene::Point::new(x as f32, y as f32),
            )
            .map(|pt| (pt.x() as f64, pt.y() as f64))
            .unwrap_or((x, y));

        let ws_weak = Rc::downgrade(self);
        show_context_menu_popover(&self.root_box, root_x, root_y, &entries, move |action| {
            let Some(ws) = ws_weak.upgrade() else {
                return;
            };
            match action {
                MenuAction::Copy => {
                    if let Some(p) = ws.pane_widgets.borrow().get(&tile_id) {
                        p.copy_selection_or_screen();
                    }
                }
                MenuAction::Paste => {
                    if let Some(p) = ws.pane_widgets.borrow().get(&tile_id) {
                        p.paste_clipboard();
                    }
                }
                MenuAction::SplitHorizontal => ws.split_pane(tile_id, true),
                MenuAction::SplitVertical => ws.split_pane(tile_id, false),
                MenuAction::NewTab => ws.add_tab(tile_id),
                MenuAction::ClearScrollback => {
                    if let Some(p) = ws.pane_widgets.borrow().get(&tile_id) {
                        p.clear_scrollback();
                    }
                }
                MenuAction::ToggleScrollbar => {
                    if let Some(p) = ws.pane_widgets.borrow().get(&tile_id) {
                        p.toggle_scrollbar();
                    }
                }
                MenuAction::ToggleReadOnly => {
                    if let Some(p) = ws.pane_widgets.borrow().get(&tile_id) {
                        p.toggle_read_only();
                    }
                    ws.refresh_all_panes();
                }
                MenuAction::SaveLayout => {
                    let _ = ws.save_current_layout();
                }
                MenuAction::LoadLayout => {
                    let _ = ws.load_saved_layout();
                }
                MenuAction::ToggleFullscreen => {
                    ws.emit_window_action(WorkspaceWindowAction::ToggleMaximizeWindow);
                }
                MenuAction::OpenSettings => {
                    ws.emit_window_action(WorkspaceWindowAction::OpenPreferences);
                }
                MenuAction::ClosePane => ws.close_pane_by_id(tile_id),
                MenuAction::ApplyPreset(preset_name) => {
                    if let Some(preset) = PresetStore::load().find_by_name(&preset_name).cloned() {
                        ws.apply_preset(&preset);
                    }
                }
                MenuAction::PluginCommand(cmd) => {
                    if let Some(p) = ws.pane_widgets.borrow().get(&tile_id) {
                        let line = format!("{}\n", cmd);
                        p.write_input(line.as_bytes());
                    }
                }
            }
        });
    }

    fn emit_window_action(&self, action: WorkspaceWindowAction) {
        if let Some(ref cb) = *self.on_window_action.borrow() {
            cb(action);
        }
    }

    pub fn rebuild_gtk_tree(self: &Rc<Self>) {
        self.rebuilding.set(true);

        for pane in self.pane_widgets.borrow().values() {
            if let Some(parent) = pane.root().parent()
                && let Ok(parent_box) = parent.downcast::<gtk4::Box>()
            {
                parent_box.remove(pane.root());
            }
        }

        while let Some(child) = self.tree_box.first_child() {
            self.tree_box.remove(&child);
        }

        let root_node_opt = self.tree.borrow().root.clone();
        if let Some(root_node) = root_node_opt {
            let built = self.build_gtk_node(&root_node);
            self.tree_box.append(&built);
        }

        self.rebuilding.set(false);
        self.schedule_pass_through_sync();
    }

    fn build_gtk_node(self: &Rc<Self>, node: &TreeNode) -> gtk4::Widget {
        match node {
            TreeNode::Pane(id) => {
                let slot = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                slot.set_hexpand(true);
                slot.set_vexpand(true);
                if let Some(pane) = self.pane_widgets.borrow().get(id) {
                    slot.append(pane.root());
                }
                slot.upcast::<gtk4::Widget>()
            }
            TreeNode::Horizontal(children) => {
                self.build_paned_chain(children, gtk4::Orientation::Horizontal)
            }
            TreeNode::Vertical(children) => {
                self.build_paned_chain(children, gtk4::Orientation::Vertical)
            }
            TreeNode::Tabs {
                children,
                active_idx,
            } => {
                let notebook = gtk4::Notebook::new();
                notebook.set_hexpand(true);
                notebook.set_vexpand(true);
                notebook.set_scrollable(true);
                notebook.set_show_border(false);

                for child in children {
                    let child_widget = self.build_gtk_node(child);
                    let tab_title = self.extract_node_title(child);
                    let tab_lbl = gtk4::Label::new(Some(&tab_title));
                    notebook.append_page(&child_widget, Some(&tab_lbl));
                }

                notebook.set_current_page(Some(*active_idx as u32));

                let child_nodes = children.clone();
                let ws_weak = Rc::downgrade(self);
                notebook.connect_switch_page(move |_nb, _page, page_num| {
                    let Some(ws) = ws_weak.upgrade() else {
                        return;
                    };
                    if ws.rebuilding.get() {
                        return;
                    }
                    if let Some(selected_node) = child_nodes.get(page_num as usize)
                        && let Some(first_pane) = first_pane_in_node(selected_node)
                    {
                        ws.tree.borrow_mut().set_active_pane(first_pane);
                        ws.refresh_all_panes();
                        ws.focus_active_pane();
                    }
                });

                notebook.upcast::<gtk4::Widget>()
            }
        }
    }

    fn build_paned_chain(
        self: &Rc<Self>,
        children: &[TreeNode],
        orientation: gtk4::Orientation,
    ) -> gtk4::Widget {
        if children.is_empty() {
            return gtk4::Box::new(orientation, 0).upcast();
        }
        if children.len() == 1 {
            return self.build_gtk_node(&children[0]);
        }

        let paned = gtk4::Paned::new(orientation);
        paned.set_hexpand(true);
        paned.set_vexpand(true);
        paned.set_wide_handle(true);
        paned.set_shrink_start_child(true);
        paned.set_shrink_end_child(true);
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(true);

        let first_widget = self.build_gtk_node(&children[0]);
        let rest_widget = self.build_paned_chain(&children[1..], orientation);

        paned.set_start_child(Some(&first_widget));
        paned.set_end_child(Some(&rest_widget));

        let is_horizontal = orientation == gtk4::Orientation::Horizontal;
        let anchor_pane_id = first_pane_in_node(&children[0]);
        let default_ratio = 1.0 / (children.len() as f64);
        let initial_ratio = self
            .tree
            .borrow()
            .split_ratio(is_horizontal, anchor_pane_id, default_ratio);

        let ratio = Rc::new(Cell::new(initial_ratio));
        let last_max = Rc::new(Cell::new(0i32));
        let updating_pos = Rc::new(Cell::new(false));

        {
            let ratio_c = Rc::clone(&ratio);
            let last_max_c = Rc::clone(&last_max);
            let updating_c = Rc::clone(&updating_pos);
            let ws_weak = Rc::downgrade(self);
            paned.connect_notify_local(Some("max-position"), move |p, _| {
                let max_pos = p.max_position();
                if max_pos > 0 {
                    last_max_c.set(max_pos);
                    let target = (max_pos as f64 * ratio_c.get()).round() as i32;
                    updating_c.set(true);
                    p.set_position(target.clamp(0, max_pos));
                    updating_c.set(false);
                    if let Some(ws) = ws_weak.upgrade() {
                        ws.schedule_pass_through_sync();
                    }
                }
            });
        }
        {
            let ratio_c = Rc::clone(&ratio);
            let last_max_c = Rc::clone(&last_max);
            let updating_c = Rc::clone(&updating_pos);
            let ws_weak = Rc::downgrade(self);
            paned.connect_notify_local(Some("position"), move |p, _| {
                if updating_c.get() {
                    return;
                }
                let max_pos = p.max_position();
                if max_pos > 0 && max_pos == last_max_c.get() {
                    let expected = (max_pos as f64 * ratio_c.get()).round() as i32;
                    if (p.position() - expected).abs() > 1 {
                        let new_ratio = (p.position() as f64 / max_pos as f64).clamp(0.05, 0.95);
                        ratio_c.set(new_ratio);
                        if let Some(ws) = ws_weak.upgrade() {
                            ws.tree
                                .borrow_mut()
                                .set_split_ratio(is_horizontal, anchor_pane_id, new_ratio);
                            ws.schedule_pass_through_sync();
                        }
                    }
                }
            });
        }

        paned.upcast::<gtk4::Widget>()
    }

    fn extract_node_title(&self, node: &TreeNode) -> String {
        if let Some(id) = first_pane_in_node(node)
            && let Some(meta) = self.tree.borrow().panes.get(&id)
        {
            return meta.title.clone();
        }
        default_shell_title()
    }

    pub fn refresh_all_panes(self: &Rc<Self>) {
        let tree = self.tree.borrow();
        let active_id = tree.active_pane;
        let top_right_id = tree.top_right_pane_id();
        let mode = self.mode.get();
        let kb_target = self.kb_target_pane.get();
        let opacity_selected = self.opacity_selected_panes.borrow();
        let opacity_confirmed = self.opacity_confirmed.get();
        let opacity_initial = matches!(mode, InteractionMode::AdjustingOpacity(_))
            && !opacity_confirmed
            && opacity_selected.is_empty()
            && kb_target.is_none();
        let win_focused = self.window_focused.get();
        let settings = self.settings.borrow().clone();
        let widgets = self.pane_widgets.borrow();

        for (&tile_id, pane) in widgets.iter() {
            let bcast_group = tree
                .panes
                .get(&tile_id)
                .map(|m| m.broadcast_group)
                .unwrap_or(0);
            if let Some(meta) = tree.panes.get(&tile_id) {
                pane.titlebar().set_title(&meta.title);
            }
            pane.set_keyboard_target(kb_target == Some(tile_id));
            let is_opacity_src = opacity_initial || opacity_selected.contains(&tile_id);
            pane.set_opacity_highlight(is_opacity_src, opacity_confirmed);
            let ctx = PaneContext {
                tile_id,
                is_active: win_focused && active_id == Some(tile_id),
                is_top_right: top_right_id == Some(tile_id),
                broadcast_group: bcast_group,
                mode,
            };
            pane.update_context_and_theme(&ctx, &settings);
        }
        drop(widgets);
        drop(tree);
        self.schedule_pass_through_sync();
    }

    pub fn focus_active_pane(&self) {
        if self.modal_blocked.get() || self.pane_selector.is_open() {
            return;
        }
        let active_id = self.tree.borrow().active_pane;
        if let Some(id) = active_id
            && let Some(pane) = self.pane_widgets.borrow().get(&id)
        {
            pane.grab_terminal_focus();
        }
    }
}

fn first_pane_in_node(node: &TreeNode) -> Option<TileId> {
    match node {
        TreeNode::Pane(id) => Some(*id),
        TreeNode::Horizontal(children) | TreeNode::Vertical(children) => {
            children.first().and_then(first_pane_in_node)
        }
        TreeNode::Tabs {
            children,
            active_idx,
        } => children
            .get(*active_idx)
            .or_else(|| children.first())
            .and_then(first_pane_in_node),
    }
}

pub fn default_shell_title() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/bin/bash".to_string())
}