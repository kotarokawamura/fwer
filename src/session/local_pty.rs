// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::ffi::CString;
use std::num::NonZeroU16;
use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use super::{SessionDriver, resolve_pid_cwd};

/// Standalone POSIX PTY session implementing [`SessionDriver`] (Section 8.2).
/// Ready to be paired with `Vt100Adapter` when replacing `vte4`.
pub struct LocalPtySession {
    master_fd: RawFd,
    child_pid: i32,
    initial_cwd: Option<PathBuf>,
    rx: Receiver<Vec<u8>>,
    wakeup_pending: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
}

impl LocalPtySession {
    pub fn spawn(
        rows: NonZeroU16,
        cols: NonZeroU16,
        working_dir: Option<&Path>,
        startup_command: Option<&str>,
        on_wakeup: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<Self, String> {
        let mut master_fd: libc::c_int = -1;
        let mut slave_fd: libc::c_int = -1;
        let ws = libc::winsize {
            ws_row: rows.get(),
            ws_col: cols.get(),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let rc = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &ws as *const libc::winsize as *mut libc::winsize,
            )
        };
        if rc != 0 {
            return Err("openpty failed".to_string());
        }

        let initial_cwd = working_dir
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::current_dir().ok());

        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "/bin/bash".to_string());

        let pid = unsafe { libc::fork() };
        if pid < 0 {
            unsafe {
                libc::close(master_fd);
                libc::close(slave_fd);
            }
            return Err("fork failed".to_string());
        }

        if pid == 0 {
            // Child process
            unsafe {
                libc::close(master_fd);
                libc::setsid();
                libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0);
                libc::dup2(slave_fd, 0);
                libc::dup2(slave_fd, 1);
                libc::dup2(slave_fd, 2);
                if slave_fd > 2 {
                    libc::close(slave_fd);
                }

                if let Some(ref dir) = initial_cwd {
                    if let Some(dir_str) = dir.to_str() {
                        if let Ok(c_dir) = CString::new(dir_str) {
                            let _ = libc::chdir(c_dir.as_ptr());
                        }
                    }
                }

                let term_k = CString::new("TERM").unwrap();
                let term_v = CString::new("xterm-256color").unwrap();
                libc::setenv(term_k.as_ptr(), term_v.as_ptr(), 1);

                let ct_k = CString::new("COLORTERM").unwrap();
                let ct_v = CString::new("truecolor").unwrap();
                libc::setenv(ct_k.as_ptr(), ct_v.as_ptr(), 1);

                let c_shell = CString::new(shell.as_str()).unwrap_or_else(|_| CString::new("/bin/sh").unwrap());
                if let Some(cmd) = startup_command.filter(|s| !s.trim().is_empty()) {
                    let c_flag = CString::new("-c").unwrap();
                    let c_cmd = CString::new(cmd).unwrap_or_else(|_| CString::new("true").unwrap());
                    let argv = [
                        c_shell.as_ptr(),
                        c_flag.as_ptr(),
                        c_cmd.as_ptr(),
                        std::ptr::null(),
                    ];
                    libc::execvp(c_shell.as_ptr(), argv.as_ptr());
                } else {
                    let argv = [c_shell.as_ptr(), std::ptr::null()];
                    libc::execvp(c_shell.as_ptr(), argv.as_ptr());
                }
                libc::_exit(127);
            }
        }

        // Parent process
        unsafe {
            libc::close(slave_fd);
        }

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let wakeup_pending = Arc::new(AtomicBool::new(false));
        let exited = Arc::new(AtomicBool::new(false));

        let reader_fd = unsafe { libc::dup(master_fd) };
        let wakeup_clone = Arc::clone(&wakeup_pending);
        let exited_clone = Arc::clone(&exited);

        thread::Builder::new()
            .name("fwer-pty-reader".to_string())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    let n = unsafe {
                        libc::read(
                            reader_fd,
                            buf.as_mut_ptr() as *mut libc::c_void,
                            buf.len(),
                        )
                    };
                    if n > 0 {
                        let chunk = buf[..n as usize].to_vec();
                        if tx.send(chunk).is_err() {
                            break;
                        }
                        if !wakeup_clone.swap(true, Ordering::AcqRel) {
                            if let Some(ref cb) = on_wakeup {
                                cb();
                            }
                        }
                    } else {
                        exited_clone.store(true, Ordering::Release);
                        if let Some(ref cb) = on_wakeup {
                            cb();
                        }
                        break;
                    }
                }
                unsafe {
                    libc::close(reader_fd);
                }
            })
            .map_err(|e| format!("failed to spawn pty reader thread: {}", e))?;

        Ok(Self {
            master_fd,
            child_pid: pid,
            initial_cwd,
            rx,
            wakeup_pending,
            exited,
        })
    }
}

impl SessionDriver for LocalPtySession {
    fn read_output(&mut self) -> Vec<u8> {
        let mut combined = Vec::new();
        while let Ok(chunk) = self.rx.try_recv() {
            combined.extend_from_slice(&chunk);
        }
        self.wakeup_pending.store(false, Ordering::Release);
        combined
    }

    fn write_input(&mut self, data: &[u8]) {
        if data.is_empty() || self.master_fd < 0 {
            return;
        }
        unsafe {
            let _ = libc::write(
                self.master_fd,
                data.as_ptr() as *const libc::c_void,
                data.len(),
            );
        }
    }

    fn resize(&mut self, rows: NonZeroU16, cols: NonZeroU16) {
        if self.master_fd < 0 {
            return;
        }
        let ws = libc::winsize {
            ws_row: rows.get(),
            ws_col: cols.get(),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            let _ = libc::ioctl(self.master_fd, libc::TIOCSWINSZ as _, &ws);
        }
    }

    fn is_alive(&mut self) -> bool {
        if self.exited.load(Ordering::Acquire) {
            return false;
        }
        let mut status: libc::c_int = 0;
        let res = unsafe { libc::waitpid(self.child_pid, &mut status, libc::WNOHANG) };
        if res == self.child_pid || res < 0 {
            self.exited.store(true, Ordering::Release);
            false
        } else {
            true
        }
    }

    fn current_working_dir(&self) -> Option<PathBuf> {
        if self.master_fd >= 0 {
            let pgrp = unsafe { libc::tcgetpgrp(self.master_fd) };
            if pgrp > 0 {
                if let Some(cwd) = resolve_pid_cwd(pgrp as u32) {
                    return Some(cwd);
                }
            }
        }
        if self.child_pid > 0 {
            if let Some(cwd) = resolve_pid_cwd(self.child_pid as u32) {
                return Some(cwd);
            }
        }
        self.initial_cwd.clone()
    }
}

impl Drop for LocalPtySession {
    fn drop(&mut self) {
        if self.master_fd >= 0 {
            unsafe {
                libc::close(self.master_fd);
            }
            self.master_fd = -1;
        }
        if self.child_pid > 0 {
            unsafe {
                libc::kill(self.child_pid, libc::SIGHUP);
                let mut status = 0;
                let _ = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
            }
        }
    }
}

#[cfg(test)]
#[path = "test/local_pty_tests.rs"]
mod local_pty_tests;
