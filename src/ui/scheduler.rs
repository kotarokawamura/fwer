// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollMode {
    EventDriven,
    ContinuousFps,
}

impl Default for PollMode {
    fn default() -> Self {
        Self::EventDriven
    }
}

impl PollMode {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "event" | "event-driven" | "event_driven" => Some(Self::EventDriven),
            "fps" | "continuous" | "polling" | "poll" => Some(Self::ContinuousFps),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RepaintScheduler {
    pub mode: PollMode,
    pub target_fps: u32,
    last_repaint: Option<Instant>,
}

impl RepaintScheduler {
    pub fn new(mode: PollMode, target_fps: u32) -> Self {
        Self {
            mode,
            target_fps: target_fps.clamp(1, 240),
            last_repaint: None,
        }
    }

    pub fn set_target_fps(&mut self, fps: u32) {
        self.target_fps = fps.clamp(1, 240);
    }

    pub fn frame_interval(&self) -> Duration {
        let fps = self.target_fps.clamp(1, 240) as u64;
        let ms = (1000 / fps).max(4);
        Duration::from_millis(ms)
    }

    pub fn next_repaint_delay(&mut self, is_minimized: bool, pty_dirty: bool) -> Option<Duration> {
        if is_minimized {
            return None;
        }
        let interval = self.frame_interval();
        match self.mode {
            PollMode::ContinuousFps => {
                self.last_repaint = Some(Instant::now());
                Some(interval)
            }
            PollMode::EventDriven => {
                if !pty_dirty {
                    return None;
                }
                let now = Instant::now();
                if let Some(prev) = self.last_repaint {
                    let elapsed = now.saturating_duration_since(prev);
                    if elapsed < interval {
                        return Some(interval - elapsed);
                    }
                }
                self.last_repaint = Some(now);
                Some(Duration::ZERO)
            }
        }
    }
}

#[cfg(test)]
#[path = "test/scheduler_tests.rs"]
mod scheduler_tests;
