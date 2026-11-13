// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use super::PreferencesWindow;
use crate::config::theme::{ThemeSettings, format_rgb_hex, parse_color};
use crate::i18n::tr;
use crate::vte::RgbColor;

/// ComboBox 上でタッチパッドスワイプやマウスホイールを回した際に選択項目が勝手に変わるのを防ぎます。
pub fn disable_combo_scroll(widget: &impl IsA<gtk4::Widget>) {
    let scroll_ctrl = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
    scroll_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    scroll_ctrl.connect_scroll(|_, _, _| glib::Propagation::Stop);
    widget.add_controller(scroll_ctrl);
}

pub fn color_status_text(raw: &str) -> String {
    if raw.trim().is_empty() {
        tr("Using Default")
    } else if parse_color(raw).is_some() {
        tr("Applied")
    } else {
        tr("Invalid Color (Fallback to Default)")
    }
}

/// 固定幅ラベル付きの水平 Box 行を生成するヘルパー
pub fn build_labeled_row(label_text: &str, width_chars: i32) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let lbl = gtk4::Label::new(Some(label_text));
    lbl.set_width_chars(width_chars);
    lbl.set_halign(gtk4::Align::Start);
    row.append(&lbl);
    row
}

/// Presets タブの Window / Panel 設定で使われる「CheckButton + Scale (`Option<f32>`)」の共通行ビルダー
#[allow(clippy::too_many_arguments)]
pub fn build_optional_scale_row(
    label_text: &str,
    check_width_px: i32,
    current_val: Option<f32>,
    default_val: f32,
    min: f64,
    max: f64,
    step: f64,
    digits: i32,
    snap_half: bool,
    on_change: impl Fn(Option<f32>) + 'static,
) -> gtk4::Box {
    let on_change = Rc::new(on_change);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

    let chk = gtk4::CheckButton::with_label(label_text);
    chk.set_width_request(check_width_px);
    chk.set_active(current_val.is_some());

    let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, min, max, step);
    scale.set_draw_value(true);
    scale.set_digits(digits);
    scale.set_value(current_val.unwrap_or(default_val) as f64);
    scale.set_hexpand(true);
    scale.set_sensitive(current_val.is_some());

    let normalize = move |v: f64| -> f32 {
        let f = v as f32;
        if snap_half {
            (f * 2.0).round() / 2.0
        } else {
            f
        }
    };

    {
        let sc = scale.clone();
        let cb = Rc::clone(&on_change);
        chk.connect_toggled(move |c| {
            let active = c.is_active();
            sc.set_sensitive(active);
            cb(if active {
                Some(normalize(sc.value()))
            } else {
                None
            });
        });
    }
    {
        let chk_c = chk.clone();
        let cb = Rc::clone(&on_change);
        scale.connect_value_changed(move |sc| {
            if chk_c.is_active() {
                cb(Some(normalize(sc.value())));
            }
        });
    }

    row.append(&chk);
    row.append(&scale);
    row
}

impl PreferencesWindow {
    pub(super) fn add_color_editor_row<F>(
        self: &Rc<Self>,
        container: &gtk4::Box,
        label_text: &str,
        initial_val: &str,
        fallback: RgbColor,
        setter: F,
    ) where
        F: Fn(&mut ThemeSettings, String) + 'static,
    {
        let setter = Rc::new(setter);
        let row = build_labeled_row(label_text, 26);

        let entry = gtk4::Entry::new();
        entry.set_text(initial_val);
        entry.set_placeholder_text(Some(&format_rgb_hex(fallback)));
        entry.set_width_chars(14);
        row.append(&entry);

        let color_dialog = gtk4::ColorDialog::new();
        color_dialog.set_with_alpha(false);
        let eff_rgb = parse_color(initial_val).unwrap_or(fallback);
        let color_btn = gtk4::ColorDialogButton::new(Some(color_dialog));
        color_btn.set_rgba(&gdk::RGBA::new(
            eff_rgb.0 as f32 / 255.0,
            eff_rgb.1 as f32 / 255.0,
            eff_rgb.2 as f32 / 255.0,
            1.0,
        ));
        row.append(&color_btn);

        let status_lbl = gtk4::Label::new(Some(&color_status_text(initial_val)));
        status_lbl.set_width_chars(18);
        status_lbl.set_halign(gtk4::Align::Start);
        row.append(&status_lbl);

        let def_btn = gtk4::Button::with_label(&tr("Default"));
        row.append(&def_btn);

        let updating = Rc::new(Cell::new(false));

        {
            let p_weak = Rc::downgrade(self);
            let setter_clone = Rc::clone(&setter);
            let status_clone = status_lbl.clone();
            let color_btn_clone = color_btn.clone();
            let upd = Rc::clone(&updating);
            entry.connect_changed(move |e| {
                if upd.get() {
                    return;
                }
                let txt = e.text().to_string();
                status_clone.set_text(&color_status_text(&txt));
                let parsed = parse_color(&txt);
                let rgb = parsed.unwrap_or(fallback);
                upd.set(true);
                color_btn_clone.set_rgba(&gdk::RGBA::new(
                    rgb.0 as f32 / 255.0,
                    rgb.1 as f32 / 255.0,
                    rgb.2 as f32 / 255.0,
                    1.0,
                ));
                upd.set(false);
                if (txt.trim().is_empty() || parsed.is_some())
                    && let Some(p) = p_weak.upgrade()
                {
                    let mut st = p.workspace.settings();
                    setter_clone(&mut st, txt);
                    p.notify_changed(st);
                }
            });
        }

        {
            let entry_clone = entry.clone();
            let status_clone = status_lbl.clone();
            let upd = Rc::clone(&updating);
            let p_weak = Rc::downgrade(self);
            let setter_clone = Rc::clone(&setter);

            color_btn.connect_rgba_notify(move |btn| {
                if upd.get() {
                    return;
                }
                let rgba = btn.rgba();
                let r = (rgba.red() * 255.0).round().clamp(0.0, 255.0) as u8;
                let g = (rgba.green() * 255.0).round().clamp(0.0, 255.0) as u8;
                let b = (rgba.blue() * 255.0).round().clamp(0.0, 255.0) as u8;
                let hex = format_rgb_hex(RgbColor(r, g, b));

                upd.set(true);
                entry_clone.set_text(&hex);
                upd.set(false);

                status_clone.set_text(&color_status_text(&hex));

                if let Some(p) = p_weak.upgrade() {
                    let mut st = p.workspace.settings();
                    setter_clone(&mut st, hex);
                    p.notify_changed(st);
                }
            });
        }

        {
            let entry_clone = entry.clone();
            def_btn.connect_clicked(move |_| {
                entry_clone.set_text("");
            });
        }

        container.append(&row);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn add_slider_row<F>(
        self: &Rc<Self>,
        container: &gtk4::Box,
        label_text: &str,
        initial_val: f64,
        min: f64,
        max: f64,
        step: f64,
        digits: i32,
        setter: F,
    ) -> gtk4::Box
    where
        F: Fn(&mut ThemeSettings, f64) + 'static,
    {
        self.add_slider_row_with_enabled(
            container,
            label_text,
            initial_val,
            min,
            max,
            step,
            digits,
            true,
            setter,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn add_slider_row_with_enabled<F>(
        self: &Rc<Self>,
        container: &gtk4::Box,
        label_text: &str,
        initial_val: f64,
        min: f64,
        max: f64,
        step: f64,
        digits: i32,
        enabled: bool,
        setter: F,
    ) -> gtk4::Box
    where
        F: Fn(&mut ThemeSettings, f64) + 'static,
    {
        let setter = Rc::new(setter);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

        let lbl = gtk4::Label::new(Some(label_text));
        lbl.set_width_chars(26);
        lbl.set_halign(gtk4::Align::Start);
        row.append(&lbl);

        let minus_btn = gtk4::Button::with_label("−");
        row.append(&minus_btn);

        let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, min, max, step);
        scale.set_digits(digits);
        scale.set_draw_value(true);
        scale.set_value(initial_val);
        scale.set_hexpand(true);
        row.append(&scale);

        let plus_btn = gtk4::Button::with_label("+");
        row.append(&plus_btn);

        if enabled {
            {
                let sc_clone = scale.clone();
                minus_btn.connect_clicked(move |_| {
                    sc_clone.set_value((sc_clone.value() - step).clamp(min, max));
                });
            }
            {
                let sc_clone = scale.clone();
                plus_btn.connect_clicked(move |_| {
                    sc_clone.set_value((sc_clone.value() + step).clamp(min, max));
                });
            }
            {
                let p_weak = Rc::downgrade(self);
                let setter_clone = Rc::clone(&setter);
                scale.connect_value_changed(move |sc| {
                    if let Some(p) = p_weak.upgrade() {
                        let mut st = p.workspace.settings();
                        setter_clone(&mut st, sc.value());
                        p.notify_changed(st);
                    }
                });
            }
        } else {
            row.add_css_class("fwer-disabled-row");
            row.set_sensitive(false);
            row.set_can_target(false);
            lbl.set_sensitive(false);
            for w in [
                minus_btn.upcast_ref::<gtk4::Widget>(),
                scale.upcast_ref::<gtk4::Widget>(),
                plus_btn.upcast_ref::<gtk4::Widget>(),
            ] {
                w.set_sensitive(false);
                w.set_can_target(false);
                w.set_focusable(false);
            }

            let disabled_lbl = gtk4::Label::new(Some(&tr("Disabled (--fps not specified)")));
            disabled_lbl.set_sensitive(false);
            row.append(&disabled_lbl);
            row.set_tooltip_text(Some(&tr("Disabled (--fps not specified)")));
        }

        container.append(&row);
        row
    }
}
