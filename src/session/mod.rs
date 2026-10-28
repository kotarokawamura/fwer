// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

pub mod local_pty;
pub mod tmux_cc;

use std::fs;
use std::num::NonZeroU16;
use std::path::PathBuf;

/// Common interface for terminal session backends (Section 8.1).
/// Used when driving a custom VTE parser (`Vt100Adapter`) or `tmux -CC` control mode.
pub trait SessionDriver {
    fn read_output(&mut self) -> Vec<u8>;
    fn write_input(&mut self, data: &[u8]);
    fn resize(&mut self, rows: NonZeroU16, cols: NonZeroU16);
    fn is_alive(&mut self) -> bool;
    fn current_working_dir(&self) -> Option<PathBuf>;
}

/// Resolves the current working directory of a process on Linux via `/proc/<pid>/cwd`.
pub fn resolve_pid_cwd(pid: u32) -> Option<PathBuf> {
    if pid == 0 {
        return None;
    }
    let link_path = format!("/proc/{}/cwd", pid);
    fs::read_link(link_path).ok().filter(|p| p.exists())
}
