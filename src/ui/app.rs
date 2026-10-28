// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;

use crate::config::prefs_ui::PreferencesWindow;
use crate::config::theme::ThemeSettings;
use crate::ui::scheduler::{PollMode, RepaintScheduler};
use crate::ui::workspace::{Workspace, WorkspaceWindowAction};

pub const RESIZE_CORNER_SIZE: f64 = 16.0;

pub fn hit_test_resize_edge(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    border: f64,
) -> Option<gdk::SurfaceEdge> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let threshold = (border + 2.0).max(4.0);
    let near_left = x <= threshold;
    let near_right = x >= width - threshold;
    let near_top = y <= threshold;
    let near_bottom = y >= height - threshold;

    if !near_left && !near_right && !near_top && !near_bottom {
        return None;
    }

    let in_top_corner = y <= RESIZE_CORNER_SIZE;
    let in_bottom_corner = y >= height - RESIZE_CORNER_SIZE;
    let in_left_corner = x <= RESIZE_CORNER_SIZE;
    let in_right_corner = x >= width - RESIZE_CORNER_SIZE;

    if (near_top && in_left_corner) || (near_left && in_top_corner) {
        Some(gdk::SurfaceEdge::NorthWest)
    } else if (near_top && in_right_corner) || (near_right && in_top_corner) {
        Some(gdk::SurfaceEdge::NorthEast)
    } else if (near_bottom && in_left_corner) || (near_left && in_bottom_corner) {
        Some(gdk::SurfaceEdge::SouthWest)
    } else if (near_bottom && in_right_corner) || (near_right && in_bottom_corner) {
        Some(gdk::SurfaceEdge::SouthEast)
    } else if near_top {
        Some(gdk::SurfaceEdge::North)
    } else if near_bottom {
        Some(gdk::SurfaceEdge::South)
    } else if near_left {
        Some(gdk::SurfaceEdge::West)
    } else {
        Some(gdk::SurfaceEdge::East)
    }
}

fn edge_cursor_name(edge: gdk::SurfaceEdge) -> &'static str {
    match edge {
        gdk::SurfaceEdge::NorthWest => "nw-resize",
        gdk::SurfaceEdge::NorthEast => "ne-resize",
        gdk::SurfaceEdge::SouthWest => "sw-resize",
        gdk::SurfaceEdge::SouthEast => "se-resize",
        gdk::SurfaceEdge::North => "n-resize",
        gdk::SurfaceEdge::South => "s-resize",
        gdk::SurfaceEdge::West => "w-resize",
        gdk::SurfaceEdge::East => "e-resize",
        _ => "default",
    }
}

pub struct FwerApp {
    pub window: gtk4::ApplicationWindow,
    pub frame_box: gtk4::Box,
    pub workspace: Rc<Workspace>,
    pub prefs_window: RefCell<Option<Rc<PreferencesWindow>>>,
    pub scheduler: RefCell<RepaintScheduler>,
}

impl FwerApp {
    pub fn build(
        app: &gtk4::Application,
        settings: ThemeSettings,
        startup_command: Option<String>,
        maximize_on_start: bool,
        open_prefs_on_start: bool,
        poll_mode: PollMode,
    ) -> Rc<Self> {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("termin")
            .decorated(false)
            .default_width(1100)
            .default_height(720)
            .width_request(400)
            .height_request(260)
            .build();

        let frame_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame_box.set_hexpand(true);
        frame_box.set_vexpand(true);
        frame_box.add_css_class("fwer-window-frame");

        let app_holder: Rc<RefCell<Option<Rc<Self>>>> = Rc::new(RefCell::new(None));
        let holder_for_ws = Rc::clone(&app_holder);

        let target_fps = settings.effective_target_fps();
        let workspace = Workspace::new(settings.clone(), startup_command, move |action| {
            let Some(app_rc) = holder_for_ws.borrow().clone() else {
                return;
            };
            match action {
                WorkspaceWindowAction::MinimizeWindow => {
                    app_rc.window.minimize();
                }
                WorkspaceWindowAction::ToggleMaximizeWindow => {
                    if app_rc.window.is_maximized() {
                        app_rc.window.unmaximize();
                    } else {
                        app_rc.window.maximize();
                    }
                }
                WorkspaceWindowAction::OpenPreferences => {
                    app_rc.open_preferences();
                }
                WorkspaceWindowAction::CloseApplication => {
                    app_rc.window.close();
                }
            }
        });

        workspace.set_fps_control_enabled(poll_mode == PollMode::ContinuousFps);
        frame_box.append(&workspace.root_box);
        window.set_child(Some(&frame_box));

        let fwer_app = Rc::new(Self {
            window,
            frame_box,
            workspace,
            prefs_window: RefCell::new(None),
            scheduler: RefCell::new(RepaintScheduler::new(poll_mode, target_fps)),
        });
        *app_holder.borrow_mut() = Some(Rc::clone(&fwer_app));

        fwer_app.update_frame_margins(&settings);
        fwer_app.setup_custom_resize_frame();

        {
            let app_weak = Rc::downgrade(&fwer_app);
            fwer_app
                .window
                .connect_notify_local(Some("maximized"), move |_, _| {
                    if let Some(a) = app_weak.upgrade() {
                        let st = a.workspace.settings();
                        a.update_frame_margins(&st);
                    }
                });
        }

        {
            let app_weak = Rc::downgrade(&fwer_app);
            fwer_app
                .window
                .connect_is_active_notify(move |win| {
                    if let Some(a) = app_weak.upgrade() {
                        a.workspace.set_window_focused(win.is_active());
                    }
                });
        }

        {
            let focus_ctrl = gtk4::EventControllerFocus::new();
            let app_weak_enter = Rc::downgrade(&fwer_app);
            focus_ctrl.connect_enter(move |_| {
                if let Some(a) = app_weak_enter.upgrade() {
                    a.workspace.set_window_focused(true);
                }
            });
            let app_weak_leave = Rc::downgrade(&fwer_app);
            focus_ctrl.connect_leave(move |_| {
                if let Some(a) = app_weak_leave.upgrade() {
                    a.workspace.set_window_focused(a.window.is_active());
                }
            });
            fwer_app.window.add_controller(focus_ctrl);
        }

        if maximize_on_start {
            fwer_app.window.maximize();
        }

        fwer_app.window.present();
        fwer_app.workspace.focus_active_pane();

        if open_prefs_on_start {
            fwer_app.open_preferences();
        }

        fwer_app
    }

    pub fn update_frame_margins(&self, settings: &ThemeSettings) {
        let bw = if self.window.is_maximized() {
            0
        } else {
            settings.effective_border_width().round() as i32
        };
        self.workspace.root_box.set_margin_top(bw);
        self.workspace.root_box.set_margin_bottom(bw);
        self.workspace.root_box.set_margin_start(bw);
        self.workspace.root_box.set_margin_end(bw);
        self.scheduler
            .borrow_mut()
            .set_target_fps(settings.effective_target_fps());
    }

    pub fn open_preferences(self: &Rc<Self>) {
        if let Some(existing) = self.prefs_window.borrow().clone() {
            existing.present();
            return;
        }
        let app_weak = Rc::downgrade(self);
        let prefs = PreferencesWindow::new(
            &self.window,
            Rc::clone(&self.workspace),
            move |updated_settings| {
                if let Some(a) = app_weak.upgrade() {
                    a.update_frame_margins(updated_settings);
                }
            },
        );
        *self.prefs_window.borrow_mut() = Some(Rc::clone(&prefs));
        prefs.present();
    }

    fn setup_custom_resize_frame(self: &Rc<Self>) {
        // Motion controller to update resize cursor near outer border
        {
            let motion = gtk4::EventControllerMotion::new();
            let app_weak = Rc::downgrade(self);
            motion.connect_motion(move |_, x, y| {
                let Some(a) = app_weak.upgrade() else {
                    return;
                };
                if a.window.is_maximized() {
                    a.window.set_cursor_from_name(None);
                    return;
                }
                let w = a.window.width() as f64;
                let h = a.window.height() as f64;
                let bw = a.workspace.settings().effective_border_width() as f64;
                if let Some(edge) = hit_test_resize_edge(x, y, w, h, bw) {
                    a.window.set_cursor_from_name(Some(edge_cursor_name(edge)));
                } else {
                    a.window.set_cursor_from_name(None);
                }
            });
            self.window.add_controller(motion);
        }

        // Click controller in Capture phase to initiate resize when dragging the outer border
        {
            let click = gtk4::GestureClick::new();
            click.set_button(1);
            click.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let app_weak = Rc::downgrade(self);
            click.connect_pressed(move |gesture, _n_press, x, y| {
                let Some(a) = app_weak.upgrade() else {
                    return;
                };
                if a.window.is_maximized() {
                    return;
                }
                let w = a.window.width() as f64;
                let h = a.window.height() as f64;
                let bw = a.workspace.settings().effective_border_width() as f64;
                if let Some(edge) = hit_test_resize_edge(x, y, w, h, bw) {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    if let Some(surface) = a.window.surface() {
                        if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
                            let device = gesture.device();
                            let timestamp = gesture.current_event_time();
                            toplevel.begin_resize(
                                edge,
                                device.as_ref(),
                                1,
                                x,
                                y,
                                timestamp,
                            );
                        }
                    }
                }
            });
            self.window.add_controller(click);
        }
    }
}
