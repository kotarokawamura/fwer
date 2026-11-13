// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::rc::Rc;

use gtk4::prelude::*;

use super::PreferencesWindow;
use crate::config::theme::snap_half_second_timeout;
use crate::i18n::tr;
use crate::vte::RgbColor;

impl PreferencesWindow {
    pub(super) fn build_titlebar_and_frame_tab(self: &Rc<Self>) -> gtk4::Widget {
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        let s = self.workspace.settings();

        self.add_color_editor_row(
            &vbox,
            &tr("Active Titlebar Background"),
            &s.title_active_bg,
            RgbColor(117, 33, 0),
            |st, v| st.title_active_bg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Active Titlebar Text"),
            &s.title_active_fg,
            RgbColor(255, 255, 255),
            |st, v| st.title_active_fg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Inactive Titlebar Background"),
            &s.title_inactive_bg,
            RgbColor(75, 75, 75),
            |st, v| st.title_inactive_bg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Inactive Titlebar Text"),
            &s.title_inactive_fg,
            RgbColor(210, 210, 210),
            |st, v| st.title_inactive_fg = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Window Frame Background"),
            &s.frame_bg_color,
            RgbColor(43, 43, 43),
            |st, v| st.frame_bg_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Window Frame Border"),
            &s.frame_border_color,
            RgbColor(90, 90, 90),
            |st, v| st.frame_border_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Pane Separator Color"),
            &s.separator_color,
            RgbColor(74, 74, 74),
            |st, v| st.separator_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Pane Separator Border Color"),
            &s.separator_border_color,
            s.effective_separator_rgb(),
            |st, v| st.separator_border_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Terminal Background Color"),
            &s.terminal_bg_color,
            RgbColor(0, 0, 0),
            |st, v| st.terminal_bg_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Terminal Cursor Color"),
            &s.cursor_color,
            RgbColor(220, 220, 220),
            |st, v| st.cursor_color = v,
        );
        self.add_color_editor_row(
            &vbox,
            &tr("Selection Highlight Color"),
            &s.selection_bg_color,
            RgbColor(230, 230, 230),
            |st, v| st.selection_bg_color = v,
        );

        vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        self.add_slider_row(
            &vbox,
            &tr("Terminal Background Opacity"),
            s.effective_terminal_bg_opacity() as f64,
            0.0,
            1.0,
            0.05,
            2,
            |st, v| st.terminal_bg_opacity = v as f32,
        );
        self.add_slider_row(
            &vbox,
            &tr("Pane Separator Width (px)"),
            s.effective_separator_width() as f64,
            1.0,
            16.0,
            1.0,
            0,
            |st, v| st.separator_width = v as f32,
        );
        self.add_slider_row(
            &vbox,
            &tr("Window Border Width (px)"),
            s.effective_border_width() as f64,
            1.0,
            20.0,
            1.0,
            0,
            |st, v| st.frame_border_width = v as f32,
        );
        let fps_enabled = self.workspace.is_fps_control_enabled();
        self.add_slider_row_with_enabled(
            &vbox,
            &tr("Target FPS"),
            s.effective_target_fps() as f64,
            1.0,
            240.0,
            5.0,
            0,
            fps_enabled,
            |st, v| st.target_fps = v.round() as u32,
        );

        let chk = gtk4::CheckButton::with_label(&tr("Show Scrollbar"));
        chk.set_active(s.show_scrollbar);
        {
            let p_weak = Rc::downgrade(self);
            chk.connect_toggled(move |c| {
                if let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    st.show_scrollbar = c.is_active();
                    p.notify_changed(st);
                }
            });
        }
        vbox.append(&chk);

        let select_pane_chk = gtk4::CheckButton::with_label(&tr("Select Panel Type on Split"));
        select_pane_chk.set_active(s.select_pane_type_on_split);
        {
            let p_weak = Rc::downgrade(self);
            select_pane_chk.connect_toggled(move |c| {
                if let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    st.select_pane_type_on_split = c.is_active();
                    p.notify_changed(st);
                }
            });
        }
        vbox.append(&select_pane_chk);

        self.add_slider_row(
            &vbox,
            &tr("Select Panel Type Timer (s)"),
            s.effective_select_pane_type_timeout_secs() as f64,
            0.0,
            10.0,
            0.5,
            1,
            |st, v| st.select_pane_type_timeout_secs = snap_half_second_timeout(v as f32),
        );

        vbox.upcast()
    }
}
