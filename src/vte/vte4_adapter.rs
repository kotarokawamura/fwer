// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

//! `vte4` implementation of [`TerminalBackend`].
//!
//! All direct `vte4` dependencies are isolated in this file so that replacing
//! the terminal engine in the future only requires swapping [`Vte4Adapter`]
//! in `src/vte/mod.rs`.

use std::cell::{Cell, RefCell};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use vte4::prelude::*;

use crate::config::theme::ThemeSettings;
use crate::session::resolve_pid_cwd;
use crate::vte::vt100_adapter::SCROLLBACK_LEN;
use crate::vte::{ONE_DARK_PALETTE_16, RgbColor, TerminalBackend};

pub struct Vte4Adapter {
    terminal: vte4::Terminal,
    zoom_scale: f64,
    child_pid: Rc<Cell<Option<i32>>>,
    child_alive: Rc<Cell<bool>>,
    initial_cwd: Option<PathBuf>,
    is_feeding_child: Rc<Cell<bool>>,
    on_user_input: Rc<RefCell<Option<Box<dyn Fn(&[u8]) + 'static>>>>,
    on_child_exited: Rc<RefCell<Option<Box<dyn Fn(i32) + 'static>>>>,
    on_scroll_changed: Rc<RefCell<Option<Box<dyn Fn(usize, usize) + 'static>>>>,
    last_colors: Option<(RgbColor, u32)>,
    last_font: Option<(String, u32)>,
}

impl Vte4Adapter {
    pub fn new(settings: &ThemeSettings) -> Self {
        let terminal = vte4::Terminal::new();
        terminal.set_hexpand(true);
        terminal.set_vexpand(true);
        terminal.set_focusable(true);
        terminal.set_can_focus(true);
        terminal.set_scrollback_lines(SCROLLBACK_LEN as libc::c_long);
        terminal.set_scroll_on_output(false);
        terminal.set_scroll_on_keystroke(true);
        terminal.set_allow_hyperlink(true);
        terminal.set_audible_bell(false);
        terminal.set_mouse_autohide(true);

        let child_pid = Rc::new(Cell::new(None));
        let child_alive = Rc::new(Cell::new(true));
        let is_feeding_child = Rc::new(Cell::new(false));
        let on_user_input: Rc<RefCell<Option<Box<dyn Fn(&[u8]) + 'static>>>> =
            Rc::new(RefCell::new(None));
        let on_child_exited: Rc<RefCell<Option<Box<dyn Fn(i32) + 'static>>>> =
            Rc::new(RefCell::new(None));
        let on_scroll_changed: Rc<RefCell<Option<Box<dyn Fn(usize, usize) + 'static>>>> =
            Rc::new(RefCell::new(None));

        // Forward user input commits for broadcast group synchronization
        {
            let is_feeding = Rc::clone(&is_feeding_child);
            let cb_ref = Rc::clone(&on_user_input);
            terminal.connect_commit(move |_term, text, _size| {
                if is_feeding.get() || text.is_empty() {
                    return;
                }
                if let Some(ref cb) = *cb_ref.borrow() {
                    cb(text.as_bytes());
                }
            });
        }

        // Track child exit
        {
            let alive_flag = Rc::clone(&child_alive);
            let cb_ref = Rc::clone(&on_child_exited);
            terminal.connect_child_exited(move |_term, status| {
                alive_flag.set(false);
                if let Some(ref cb) = *cb_ref.borrow() {
                    cb(status);
                }
            });
        }

        // Track scroll adjustment changes for the scroll offset badge
        if let Some(adj) = terminal.vadjustment() {
            let cb_ref1 = Rc::clone(&on_scroll_changed);
            adj.connect_value_changed(move |a| {
                let (offset, max_offset) = compute_adjustment_offsets(a);
                if let Some(ref cb) = *cb_ref1.borrow() {
                    cb(offset, max_offset);
                }
            });

            let cb_ref2 = Rc::clone(&on_scroll_changed);
            adj.connect_changed(move |a| {
                let (offset, max_offset) = compute_adjustment_offsets(a);
                if let Some(ref cb) = *cb_ref2.borrow() {
                    cb(offset, max_offset);
                }
            });
        }

        let mut adapter = Self {
            terminal,
            zoom_scale: 1.0,
            child_pid,
            child_alive,
            initial_cwd: None,
            is_feeding_child,
            on_user_input,
            on_child_exited,
            on_scroll_changed,
            last_colors: None,
            last_font: None,
        };
        adapter.apply_theme(settings);
        adapter
    }
}

fn compute_adjustment_offsets(adj: &gtk4::Adjustment) -> (usize, usize) {
    let lower = adj.lower();
    let upper = adj.upper();
    let page_size = adj.page_size();
    let value = adj.value();

    let max_scroll = (upper - page_size - lower).max(0.0).round() as usize;
    let current_offset_from_bottom = (upper - page_size - value).max(0.0).round() as usize;
    (current_offset_from_bottom.min(max_scroll), max_scroll)
}

fn rgb_to_gdk_rgba(rgb: RgbColor, alpha: f32) -> gdk::RGBA {
    gdk::RGBA::new(
        rgb.0 as f32 / 255.0,
        rgb.1 as f32 / 255.0,
        rgb.2 as f32 / 255.0,
        alpha.clamp(0.0, 1.0),
    )
}

impl TerminalBackend for Vte4Adapter {
    fn widget(&self) -> gtk4::Widget {
        self.terminal.clone().upcast::<gtk4::Widget>()
    }

    fn spawn_shell(
        &mut self,
        working_dir: Option<&Path>,
        startup_command: Option<&str>,
    ) -> Result<(), String> {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "/bin/bash".to_string());

        let cwd_path = working_dir
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::current_dir().ok());
        self.initial_cwd = cwd_path.clone();

        let cwd_str = cwd_path
            .as_ref()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string());

        let mut argv_owned: Vec<String> = vec![shell];
        if let Some(cmd) = startup_command {
            if !cmd.trim().is_empty() {
                argv_owned.push("-c".to_string());
                argv_owned.push(cmd.to_string());
            }
        }
        let argv_refs: Vec<&str> = argv_owned.iter().map(|s| s.as_str()).collect();

        let mut env_owned: Vec<String> = std::env::vars()
            .filter(|(k, _)| k != "TERM" && k != "COLORTERM" && k != "PWD")
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        env_owned.push("TERM=xterm-256color".to_string());
        env_owned.push("COLORTERM=truecolor".to_string());
        if let Some(ref dir) = cwd_str {
            env_owned.push(format!("PWD={}", dir));
        }
        let env_refs: Vec<&str> = env_owned.iter().map(|s| s.as_str()).collect();

        let pid_slot = Rc::clone(&self.child_pid);
        let alive_slot = Rc::clone(&self.child_alive);
        alive_slot.set(true);

        self.terminal.spawn_async(
            vte4::PtyFlags::DEFAULT,
            cwd_str.as_deref(),
            &argv_refs,
            &env_refs,
            glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            gio::Cancellable::NONE,
            move |res| match res {
                Ok(pid) => {
                    pid_slot.set(Some(pid.0));
                }
                Err(err) => {
                    eprintln!("[fwer] Failed to spawn child process in VTE: {}", err);
                    alive_slot.set(false);
                }
            },
        );

        Ok(())
    }

    fn write_input(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.scroll_to_bottom();
        self.is_feeding_child.set(true);
        self.terminal.feed_child(data);
        self.is_feeding_child.set(false);
    }

    fn process_bytes(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.terminal.feed(data);
        }
    }

    fn has_selection(&self) -> bool {
        self.terminal.has_selection()
    }

    fn copy_selection_or_screen(&self) {
        if self.terminal.has_selection() {
            self.terminal.copy_clipboard_format(vte4::Format::Text);
        } else {
            self.terminal.select_all();
            self.terminal.copy_clipboard_format(vte4::Format::Text);
            self.terminal.unselect_all();
        }
    }

    fn paste_clipboard(&self) {
        self.terminal.paste_clipboard();
    }

    fn clear_scrollback(&mut self) {
        self.terminal.set_scrollback_lines(0);
        self.terminal
            .set_scrollback_lines(SCROLLBACK_LEN as libc::c_long);
    }

    fn scroll_lines(&mut self, delta_lines: i32) {
        if let Some(adj) = self.terminal.vadjustment() {
            let lower = adj.lower();
            let max_val = (adj.upper() - adj.page_size()).max(lower);
            let next = (adj.value() + delta_lines as f64).clamp(lower, max_val);
            adj.set_value(next);
        }
    }

    fn scroll_page(&mut self, up: bool) {
        if let Some(adj) = self.terminal.vadjustment() {
            let half_page = (adj.page_size() / 2.0).max(1.0);
            let delta = if up { -half_page } else { half_page };
            let lower = adj.lower();
            let max_val = (adj.upper() - adj.page_size()).max(lower);
            adj.set_value((adj.value() + delta).clamp(lower, max_val));
        }
    }

    fn scroll_to_top(&mut self) {
        if let Some(adj) = self.terminal.vadjustment() {
            adj.set_value(adj.lower());
        }
    }

    fn scroll_to_bottom(&mut self) {
        if let Some(adj) = self.terminal.vadjustment() {
            let max_val = (adj.upper() - adj.page_size()).max(adj.lower());
            adj.set_value(max_val);
        }
    }

    fn scroll_offset_info(&self) -> (usize, usize) {
        if let Some(adj) = self.terminal.vadjustment() {
            compute_adjustment_offsets(&adj)
        } else {
            (0, 0)
        }
    }

    fn vadjustment(&self) -> Option<gtk4::Adjustment> {
        self.terminal.vadjustment()
    }

    fn apply_theme(&mut self, settings: &ThemeSettings) {
        let bg_rgb = settings.effective_terminal_bg_rgb();
        let opacity = settings.effective_terminal_bg_opacity();
        let color_key = (bg_rgb, opacity.to_bits());

        if self.last_colors != Some(color_key) {
            self.last_colors = Some(color_key);
            let fg = rgb_to_gdk_rgba(RgbColor::DEFAULT_FG, 1.0);
            let bg = rgb_to_gdk_rgba(bg_rgb, opacity);

            let palette_storage: Vec<gdk::RGBA> = ONE_DARK_PALETTE_16
                .iter()
                .map(|&c| rgb_to_gdk_rgba(c, 1.0))
                .collect();
            let palette_refs: Vec<&gdk::RGBA> = palette_storage.iter().collect();

            self.terminal.set_clear_background(true);
            self.terminal
                .set_colors(Some(&fg), Some(&bg), &palette_refs);

            let cursor_bg = gdk::RGBA::new(220.0 / 255.0, 220.0 / 255.0, 220.0 / 255.0, 0.8);
            self.terminal.set_color_cursor(Some(&cursor_bg));
        }

        let family = settings.effective_font_family();
        let term_size = settings.effective_terminal_font_size();
        let font_key = (family.clone(), term_size.to_bits());

        if self.last_font != Some(font_key) {
            self.last_font = Some((family.clone(), term_size.to_bits()));
            let size_pt = (term_size as f64 * 0.75).max(6.0);
            let mut font_desc = pango::FontDescription::new();
            font_desc.set_family(&family);
            font_desc.set_size((size_pt * pango::SCALE as f64) as i32);
            self.terminal.set_font(Some(&font_desc));
            self.terminal.set_font_scale(self.zoom_scale);
        }
    }

    fn set_zoom_scale(&mut self, scale: f64) {
        let clamped = scale.clamp(0.4, 3.5);
        self.zoom_scale = clamped;
        self.terminal.set_font_scale(clamped);
    }

    fn zoom_scale(&self) -> f64 {
        self.zoom_scale
    }

    fn current_working_dir(&self) -> Option<PathBuf> {
        // 1. Foreground process group leader of the PTY master fd
        if let Some(pty) = self.terminal.pty() {
            let raw_fd = pty.fd().as_raw_fd();
            let pgrp = unsafe { libc::tcgetpgrp(raw_fd) };
            if pgrp > 0 {
                if let Some(cwd) = resolve_pid_cwd(pgrp as u32) {
                    return Some(cwd);
                }
            }
        }
        // 2. Direct child shell PID
        if let Some(pid) = self.child_pid.get() {
            if pid > 0 {
                if let Some(cwd) = resolve_pid_cwd(pid as u32) {
                    return Some(cwd);
                }
            }
        }
        // 3. Initial working directory fallback
        self.initial_cwd.clone()
    }

    fn is_alive(&self) -> bool {
        self.child_alive.get()
    }

    fn dimensions(&self) -> (u16, u16) {
        let rows = self.terminal.row_count().max(1) as u16;
        let cols = self.terminal.column_count().max(1) as u16;
        (rows, cols)
    }

    fn set_on_user_input(&mut self, cb: Box<dyn Fn(&[u8]) + 'static>) {
        *self.on_user_input.borrow_mut() = Some(cb);
    }

    fn set_on_child_exited(&mut self, cb: Box<dyn Fn(i32) + 'static>) {
        *self.on_child_exited.borrow_mut() = Some(cb);
    }

    fn set_on_scroll_changed(&mut self, cb: Box<dyn Fn(usize, usize) + 'static>) {
        *self.on_scroll_changed.borrow_mut() = Some(cb);
    }
}
