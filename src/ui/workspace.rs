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

use crate::config::layout_preset::LayoutNode;
use crate::config::theme::{ThemeSettings, format_rgb_hex};
use crate::menus::MenuManager;
use crate::menus::renderer::show_context_menu_popover;
use crate::menus::types::{MenuAction, MenuContext, MenuTargetKind};
use crate::ui::pane::{PaneEvent, PaneShortcutAction, TerminalPaneWidget};
use crate::ui::state::{InteractionMode, PaneContext, TileId};
use crate::ui::titlebar::{TitlebarEvent, next_mode_on_grip_click};
use crate::ui::tree_ops::{TreeNode, WorkspaceTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceWindowAction {
    MinimizeWindow,
    ToggleMaximizeWindow,
    OpenPreferences,
    CloseApplication,
}

pub struct Workspace {
    pub root_box: gtk4::Box,
    pub tree: RefCell<WorkspaceTree>,
    pane_widgets: RefCell<HashMap<TileId, Rc<TerminalPaneWidget>>>,
    mode: Cell<InteractionMode>,
    modal_blocked: Cell<bool>,
    window_focused: Cell<bool>,
    settings: RefCell<ThemeSettings>,
    pub menu_manager: RefCell<MenuManager>,
    css_provider: gtk4::CssProvider,
    last_global_css: RefCell<String>,
    rebuilding: Cell<bool>,
    fps_control_enabled: Cell<bool>,
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

        let css_provider = gtk4::CssProvider::new();
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &css_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let default_title = default_shell_title();
        let (tree, first_id) = WorkspaceTree::with_initial_pane(&default_title, startup_command.clone());

        let ws = Rc::new(Self {
            root_box,
            tree: RefCell::new(tree),
            pane_widgets: RefCell::new(HashMap::new()),
            mode: Cell::new(InteractionMode::Normal),
            modal_blocked: Cell::new(false),
            window_focused: Cell::new(true),
            settings: RefCell::new(initial_settings),
            menu_manager: RefCell::new(MenuManager::new()),
            css_provider,
            last_global_css: RefCell::new(String::new()),
            rebuilding: Cell::new(false),
            fps_control_enabled: Cell::new(false),
            on_window_action: RefCell::new(Some(Box::new(on_window_action))),
        });

        ws.update_global_css();
        ws.spawn_and_register_pane(first_id, &default_title, None, startup_command.as_deref());
        ws.rebuild_gtk_tree();
        ws.refresh_all_panes();
        ws.focus_active_pane();

        ws
    }

    pub fn set_window_focused(&self, focused: bool) {
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
            self.mode.set(InteractionMode::BlockedByModal);
        } else if self.mode.get() == InteractionMode::BlockedByModal {
            self.mode.set(InteractionMode::Normal);
        }
        self.refresh_all_panes();
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
        let frame_bg = s.effective_frame_bg_rgb();
        let frame_border = format_rgb_hex(s.effective_frame_border_rgb());
        let opacity = s.effective_terminal_bg_opacity();
        let ui_font_px = s.effective_ui_font_size();
        let font_family = s.effective_font_family();

        let css = format!(
            ".fwer-window-frame {{
                background-color: rgba({}, {}, {}, {:.3});
                border: 1px solid {frame_border};
             }}
             .fwer-workspace paned > separator {{
                background-image: none;
                background-color: {sep_hex};
                border: 1px solid {sep_border_hex};
                box-shadow: none;
                outline: none;
                min-width: {sep_w}px;
                min-height: {sep_w}px;
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
             }}
             .fwer-font-card-selected {{
                background-color: #23372a;
                border: 2px solid #4ade80;
                border-radius: 4px;
                padding: 6px 8px;
             }}
             .fwer-pane-moving-source {{
                border: 2px solid rgb(250, 204, 21);
             }}
             .fwer-pane-broadcast-source {{
                border: 2px solid rgb(56, 189, 248);
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
                font-size: {ui_font_px}px;
             }}
             .fwer-context-popover contents {{
                background-color: #252526;
                color: #e6e6e6;
                border: 1px solid #5a5a5a;
                border-radius: 6px;
                font-family: \"{font_family}\", monospace;
                font-size: {ui_font_px}px;
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
                font-size: {:.1}px;
                margin-left: 16px;
             }}",
            frame_bg.0,
            frame_bg.1,
            frame_bg.2,
            opacity,
            (ui_font_px - 1.0).max(8.0)
        );
        if *self.last_global_css.borrow() != css {
            *self.last_global_css.borrow_mut() = css.clone();
            self.css_provider.load_from_string(&css);
        }
    }

    fn spawn_and_register_pane(
        self: &Rc<Self>,
        tile_id: TileId,
        title: &str,
        working_dir: Option<&Path>,
        startup_command: Option<&str>,
    ) -> Rc<TerminalPaneWidget> {
        let ws_weak = Rc::downgrade(self);
        let settings = self.settings.borrow().clone();
        let pane = TerminalPaneWidget::new(
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
        self.pane_widgets.borrow_mut().insert(tile_id, Rc::clone(&pane));
        pane
    }

    fn handle_pane_event(self: &Rc<Self>, event: PaneEvent) {
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
                        } else {
                            self.tree
                                .borrow_mut()
                                .link_broadcast_panes(src_id, tile_id);
                            if !keep_selecting {
                                self.mode.set(InteractionMode::Normal);
                            }
                        }
                        self.refresh_all_panes();
                        self.focus_active_pane();
                    }
                    _ => {}
                }
            }
            PaneEvent::OpenPaneContextMenu { tile_id, x, y } => {
                self.open_context_menu_for(MenuTargetKind::Pane, tile_id, x, y);
            }
            PaneEvent::ShortcutTriggered { tile_id, action } => {
                self.execute_shortcut_action(tile_id, action);
            }
            PaneEvent::CancelSpecialMode => {
                self.mode.set(InteractionMode::Normal);
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
        if self.modal_blocked.get() {
            return;
        }
        match event {
            TitlebarEvent::FocusPane(tile_id) => {
                if matches!(
                    self.mode.get(),
                    InteractionMode::MovingPane(_) | InteractionMode::SelectingBroadcast(_)
                ) {
                    self.handle_pane_event(PaneEvent::ClickedInSpecialMode {
                        tile_id,
                        keep_selecting: false,
                    });
                    return;
                }
                self.commit_any_active_title_edit();
                self.window_focused.set(true);
                self.tree.borrow_mut().set_active_pane(tile_id);
                self.refresh_all_panes();
                self.focus_active_pane();
            }
            TitlebarEvent::ToggleMoveMode(tile_id) => {
                self.commit_any_active_title_edit();
                let next = next_mode_on_grip_click(self.mode.get(), tile_id, false);
                self.mode.set(next);
                self.refresh_all_panes();
            }
            TitlebarEvent::ToggleBroadcastSelectMode(tile_id) => {
                self.commit_any_active_title_edit();
                let next = next_mode_on_grip_click(self.mode.get(), tile_id, true);
                self.mode.set(next);
                self.refresh_all_panes();
            }
            TitlebarEvent::StartEditingTitle(tile_id) => {
                self.commit_any_active_title_edit();
                self.mode.set(InteractionMode::EditingTitle(tile_id));
                self.refresh_all_panes();
            }
            TitlebarEvent::FinishEditingTitle(tile_id, new_title) => {
                if let Some(meta) = self.tree.borrow_mut().panes.get_mut(&tile_id) {
                    if !new_title.trim().is_empty() {
                        meta.title = new_title.trim().to_string();
                    }
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
        }
    }

    pub fn split_pane(self: &Rc<Self>, target_id: TileId, horizontal: bool) {
        let inherited_cwd = self
            .pane_widgets
            .borrow()
            .get(&target_id)
            .and_then(|p| p.current_working_dir());
        let title = default_shell_title();
        let new_id_opt = self
            .tree
            .borrow_mut()
            .split_pane(target_id, horizontal, &title, None);

        if let Some(new_id) = new_id_opt {
            self.spawn_and_register_pane(new_id, &title, inherited_cwd.as_deref(), None);
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
        let new_id_opt = self.tree.borrow_mut().add_tab(target_id, &title, None);

        if let Some(new_id) = new_id_opt {
            self.spawn_and_register_pane(new_id, &title, inherited_cwd.as_deref(), None);
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
            if let Some(parent) = removed.root.parent() {
                if let Ok(parent_box) = parent.downcast::<gtk4::Box>() {
                    parent_box.remove(&removed.root);
                }
            }
        }
        if is_empty {
            self.emit_window_action(WorkspaceWindowAction::CloseApplication);
            return;
        }
        self.mode.set(InteractionMode::Normal);
        self.rebuild_gtk_tree();
        self.refresh_all_panes();
        self.focus_active_pane();
    }

    pub fn save_current_layout(&self) -> Result< std::path::PathBuf, String> {
        let tree = self.tree.borrow();
        let node = LayoutNode::from_tree(&tree).ok_or_else(|| "Empty workspace tree".to_string())?;
        node.save_to_disk()
    }

    pub fn load_saved_layout(self: &Rc<Self>) -> Result<(), String> {
        let node = LayoutNode::load_from_disk()?;
        let new_tree = node.build_tree();

        // Detach all existing pane widgets
        for pane in self.pane_widgets.borrow().values() {
            if let Some(parent) = pane.root.parent() {
                if let Ok(parent_box) = parent.downcast::<gtk4::Box>() {
                    parent_box.remove(&pane.root);
                }
            }
        }
        self.pane_widgets.borrow_mut().clear();

        let panes_to_spawn: Vec<(TileId, String, Option<String>)> = new_tree
            .ordered_pane_ids()
            .into_iter()
            .filter_map(|id| {
                new_tree
                    .panes
                    .get(&id)
                    .map(|m| (id, m.title.clone(), m.command.clone()))
            })
            .collect();

        *self.tree.borrow_mut() = new_tree;
        self.mode.set(InteractionMode::Normal);

        for (id, title, cmd) in panes_to_spawn {
            self.spawn_and_register_pane(id, &title, None, cmd.as_deref());
        }

        self.rebuild_gtk_tree();
        self.refresh_all_panes();
        self.focus_active_pane();
        Ok(())
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
        let ctx = MenuContext {
            target_kind,
            tile_id,
            has_selection: pane.has_selection(),
            shortcuts: self.settings.borrow().shortcuts.clone(),
        };
        let entries = self.menu_manager.borrow().build_entries(&ctx);

        let anchor_widget: gtk4::Widget = match target_kind {
            MenuTargetKind::Pane => pane.root.clone().upcast(),
            MenuTargetKind::Titlebar => pane.titlebar.root.clone().upcast(),
        };

        let ws_weak = Rc::downgrade(self);
        show_context_menu_popover(&anchor_widget, x, y, &entries, move |action| {
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

        // Detach all pane widgets from their old slot boxes so they can be re-parented cleanly
        for pane in self.pane_widgets.borrow().values() {
            if let Some(parent) = pane.root.parent() {
                if let Ok(parent_box) = parent.downcast::<gtk4::Box>() {
                    parent_box.remove(&pane.root);
                }
            }
        }

        while let Some(child) = self.root_box.first_child() {
            self.root_box.remove(&child);
        }

        let root_node_opt = self.tree.borrow().root.clone();
        if let Some(root_node) = root_node_opt {
            let built = self.build_gtk_node(&root_node);
            self.root_box.append(&built);
        }

        self.rebuilding.set(false);
    }

    fn build_gtk_node(self: &Rc<Self>, node: &TreeNode) -> gtk4::Widget {
        match node {
            TreeNode::Pane(id) => {
                let slot = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                slot.set_hexpand(true);
                slot.set_vexpand(true);
                if let Some(pane) = self.pane_widgets.borrow().get(id) {
                    slot.append(&pane.root);
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
                    if let Some(selected_node) = child_nodes.get(page_num as usize) {
                        if let Some(first_pane) = first_pane_in_node(selected_node) {
                            ws.tree.borrow_mut().set_active_pane(first_pane);
                            ws.refresh_all_panes();
                            ws.focus_active_pane();
                        }
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

        let initial_ratio = 1.0 / (children.len() as f64);
        let ratio = Rc::new(Cell::new(initial_ratio));
        let last_max = Rc::new(Cell::new(0i32));
        let updating_pos = Rc::new(Cell::new(false));

        {
            let ratio_c = Rc::clone(&ratio);
            let last_max_c = Rc::clone(&last_max);
            let updating_c = Rc::clone(&updating_pos);
            paned.connect_notify_local(Some("max-position"), move |p, _| {
                let max_pos = p.max_position();
                if max_pos > 0 {
                    last_max_c.set(max_pos);
                    let target = (max_pos as f64 * ratio_c.get()).round() as i32;
                    updating_c.set(true);
                    p.set_position(target.clamp(0, max_pos));
                    updating_c.set(false);
                }
            });
        }
        {
            let ratio_c = Rc::clone(&ratio);
            let last_max_c = Rc::clone(&last_max);
            let updating_c = Rc::clone(&updating_pos);
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
                    }
                }
            });
        }

        paned.upcast::<gtk4::Widget>()
    }

    fn extract_node_title(&self, node: &TreeNode) -> String {
        if let Some(id) = first_pane_in_node(node) {
            if let Some(meta) = self.tree.borrow().panes.get(&id) {
                return meta.title.clone();
            }
        }
        default_shell_title()
    }

    pub fn refresh_all_panes(&self) {
        let tree = self.tree.borrow();
        let active_id = tree.active_pane;
        let top_right_id = tree.top_right_pane_id();
        let mode = self.mode.get();
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
                pane.titlebar.set_title(&meta.title);
            }
            let ctx = PaneContext {
                tile_id,
                is_active: win_focused && active_id == Some(tile_id),
                is_top_right: top_right_id == Some(tile_id),
                broadcast_group: bcast_group,
                mode,
            };
            pane.update_context_and_theme(&ctx, &settings);
        }
    }

    pub fn focus_active_pane(&self) {
        if self.modal_blocked.get() {
            return;
        }
        let active_id = self.tree.borrow().active_pane;
        if let Some(id) = active_id {
            if let Some(pane) = self.pane_widgets.borrow().get(&id) {
                pane.grab_terminal_focus();
            }
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
