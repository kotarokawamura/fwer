// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;

use crate::menus::types::{MenuAction, MenuEntry};

pub fn show_context_menu_popover<F>(
    parent: &impl IsA<gtk4::Widget>,
    x: f64,
    y: f64,
    entries: &[MenuEntry],
    on_action: F,
) where
    F: Fn(MenuAction) + 'static,
{
    let popover = gtk4::Popover::new();
    popover.set_has_arrow(false);
    popover.set_autohide(true);
    popover.set_parent(parent);
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.add_css_class("fwer-context-popover");

    let root_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    root_box.set_margin_top(4);
    root_box.set_margin_bottom(4);
    root_box.set_margin_start(4);
    root_box.set_margin_end(4);

    let on_action_rc: Rc<dyn Fn(MenuAction)> = Rc::new(on_action);
    populate_menu_entries(&root_box, entries, &popover, &on_action_rc, 0);

    popover.set_child(Some(&root_box));

    popover.connect_closed(|pop| {
        pop.unparent();
    });

    popover.popup();
}

fn populate_menu_entries(
    container: &gtk4::Box,
    entries: &[MenuEntry],
    popover: &gtk4::Popover,
    on_action: &Rc<dyn Fn(MenuAction)>,
    depth: usize,
) {
    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
                sep.set_margin_top(3);
                sep.set_margin_bottom(3);
                container.append(&sep);
            }
            MenuEntry::Item {
                icon,
                label,
                shortcut,
                action,
                enabled,
            } => {
                let btn = gtk4::Button::new();
                btn.add_css_class("flat");
                btn.add_css_class("fwer-menu-item");
                btn.set_sensitive(*enabled);

                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                row.set_margin_start((depth * 14) as i32);

                let left_text = if icon.is_empty() {
                    label.clone()
                } else {
                    format!("{}  {}", icon, label)
                };
                let lbl = gtk4::Label::new(Some(&left_text));
                lbl.set_halign(gtk4::Align::Start);
                lbl.set_hexpand(true);
                row.append(&lbl);

                if let Some(sc) = shortcut.as_ref().filter(|s| !s.trim().is_empty()) {
                    let sc_lbl = gtk4::Label::new(Some(sc));
                    sc_lbl.add_css_class("fwer-menu-shortcut");
                    sc_lbl.set_halign(gtk4::Align::End);
                    row.append(&sc_lbl);
                }

                btn.set_child(Some(&row));

                let act = action.clone();
                let pop_clone = popover.clone();
                let cb = Rc::clone(on_action);
                btn.connect_clicked(move |_| {
                    pop_clone.popdown();
                    cb(act.clone());
                });

                container.append(&btn);
            }
            MenuEntry::SubMenu {
                icon,
                label,
                entries: sub_entries,
            } => {
                let header_btn = gtk4::Button::new();
                header_btn.add_css_class("flat");
                header_btn.add_css_class("fwer-menu-item");

                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                row.set_margin_start((depth * 14) as i32);

                let title_text = if icon.is_empty() {
                    label.clone()
                } else {
                    format!("{}  {}", icon, label)
                };
                let lbl = gtk4::Label::new(Some(&title_text));
                lbl.set_halign(gtk4::Align::Start);
                lbl.set_hexpand(true);
                row.append(&lbl);

                let arrow = gtk4::Label::new(Some("▾"));
                arrow.set_halign(gtk4::Align::End);
                row.append(&arrow);
                header_btn.set_child(Some(&row));

                let revealer = gtk4::Revealer::new();
                revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
                revealer.set_reveal_child(false);

                let sub_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                populate_menu_entries(&sub_box, sub_entries, popover, on_action, depth + 1);
                revealer.set_child(Some(&sub_box));

                let rev_clone = revealer.clone();
                header_btn.connect_clicked(move |_| {
                    rev_clone.set_reveal_child(!rev_clone.reveals_child());
                });

                container.append(&header_btn);
                container.append(&revealer);
            }
        }
    }
}
