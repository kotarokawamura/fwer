// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::collections::VecDeque;
use std::num::NonZeroU16;
use std::path::PathBuf;

use super::SessionDriver;

/// Skeleton implementation for `tmux -CC` control mode integration (Section 8.3).
pub struct TmuxControlSession {
    pub pane_id: String,
    pub rows: NonZeroU16,
    pub cols: NonZeroU16,
    pub alive: bool,
    pub cwd: Option<PathBuf>,
    pub output_queue: VecDeque<u8>,
    pub sent_commands: Vec<String>,
}

impl TmuxControlSession {
    pub fn new(pane_id: impl Into<String>, rows: NonZeroU16, cols: NonZeroU16) -> Self {
        Self {
            pane_id: pane_id.into(),
            rows,
            cols,
            alive: true,
            cwd: None,
            output_queue: VecDeque::new(),
            sent_commands: Vec::new(),
        }
    }

    pub fn push_simulated_output(&mut self, bytes: &[u8]) {
        self.output_queue.extend(bytes.iter().copied());
    }
}

impl SessionDriver for TmuxControlSession {
    fn read_output(&mut self) -> Vec<u8> {
        self.output_queue.drain(..).collect()
    }

    fn write_input(&mut self, data: &[u8]) {
        if data.is_empty() || !self.alive {
            return;
        }
        let hex_parts: Vec<String> = data.iter().map(|b| format!("{:02x}", b)).collect();
        self.sent_commands
            .push(format!("send-keys -t {} -H {}", self.pane_id, hex_parts.join(" ")));
    }

    fn resize(&mut self, rows: NonZeroU16, cols: NonZeroU16) {
        self.rows = rows;
        self.cols = cols;
        self.sent_commands.push(format!(
            "refresh-client -C {}x{}",
            cols.get(),
            rows.get()
        ));
    }

    fn is_alive(&mut self) -> bool {
        self.alive
    }

    fn current_working_dir(&self) -> Option<PathBuf> {
        self.cwd.clone()
    }
}

#[cfg(test)]
#[path = "test/tmux_cc_tests.rs"]
mod tmux_cc_tests;
