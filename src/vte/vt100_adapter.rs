// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

//! Pure-Rust VT100/ANSI screen buffer and logical-line reflow adapter.
//!
//! This module provides `Vt100Adapter` as specified in Section 9 of `SPECIFICATION.md`,
//! including 5000-line scrollback, logical-line extraction across wrapped rows,
//! ANSI SGR re-serialization, resize reflow, and scrollback clearing.
//! It can also serve as the core state machine when replacing `vte4` with a custom GTK4
//! `DrawingArea` renderer.

use std::collections::VecDeque;

use super::{CursorState, RenderableCell, RgbColor, ansi_idx_to_rgb};

pub const SCROLLBACK_LEN: usize = 5000;

#[derive(Debug, Clone)]
pub struct PhysicalRow {
    pub cells: Vec<RenderableCell>,
    pub wrapped: bool,
}

impl PhysicalRow {
    pub fn new(cols: u16) -> Self {
        Self {
            cells: vec![RenderableCell::default(); cols as usize],
            wrapped: false,
        }
    }
}

#[derive(Debug, Clone)]
struct PenState {
    fg: RgbColor,
    bg: RgbColor,
    bold: bool,
    underline: bool,
    inverse: bool,
}

impl Default for PenState {
    fn default() -> Self {
        Self {
            fg: RgbColor::DEFAULT_FG,
            bg: RgbColor::DEFAULT_BG,
            bold: false,
            underline: false,
            inverse: false,
        }
    }
}

pub struct Vt100Adapter {
    rows: u16,
    cols: u16,
    scrollback: VecDeque<PhysicalRow>,
    grid: Vec<PhysicalRow>,
    alt_grid: Vec<PhysicalRow>,
    cursor_row: u16,
    cursor_col: u16,
    saved_main_cursor: (u16, u16),
    cursor_visible: bool,
    alternate_screen: bool,
    bracketed_paste: bool,
    pen: PenState,
    pending_wrap: bool,
    generation: u64,
    utf8_buf: Vec<u8>,
    esc_buf: Vec<u8>,
    in_escape: bool,
}

impl Vt100Adapter {
    pub fn new(rows: u16, cols: u16) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            rows,
            cols,
            scrollback: VecDeque::new(),
            grid: (0..rows).map(|_| PhysicalRow::new(cols)).collect(),
            alt_grid: (0..rows).map(|_| PhysicalRow::new(cols)).collect(),
            cursor_row: 0,
            cursor_col: 0,
            saved_main_cursor: (0, 0),
            cursor_visible: true,
            alternate_screen: false,
            bracketed_paste: false,
            pen: PenState::default(),
            pending_wrap: false,
            generation: 0,
            utf8_buf: Vec::new(),
            esc_buf: Vec::new(),
            in_escape: false,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn dimensions(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    pub fn alternate_screen(&self) -> bool {
        self.alternate_screen
    }

    pub fn bracketed_paste_enabled(&self) -> bool {
        self.bracketed_paste
    }

    pub fn cursor_state(&self) -> CursorState {
        CursorState {
            row: self.cursor_row.min(self.rows.saturating_sub(1)),
            col: self.cursor_col.min(self.cols.saturating_sub(1)),
            visible: self.cursor_visible,
        }
    }

    pub fn scrollback_len(&self) -> usize {
        if self.alternate_screen {
            0
        } else {
            self.scrollback.len()
        }
    }

    pub fn process_bytes(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        for &b in data {
            self.process_byte(b);
        }
    }

    fn active_grid_mut(&mut self) -> &mut Vec<PhysicalRow> {
        if self.alternate_screen {
            &mut self.alt_grid
        } else {
            &mut self.grid
        }
    }

    fn active_grid(&self) -> &Vec<PhysicalRow> {
        if self.alternate_screen {
            &self.alt_grid
        } else {
            &self.grid
        }
    }

    fn process_byte(&mut self, b: u8) {
        if self.in_escape {
            self.esc_buf.push(b);
            if self.is_escape_complete() {
                let seq = std::mem::take(&mut self.esc_buf);
                self.in_escape = false;
                self.handle_escape_sequence(&seq);
            }
            return;
        }

        if b == 0x1B {
            if !self.utf8_buf.is_empty() {
                self.utf8_buf.clear();
            }
            self.in_escape = true;
            self.esc_buf.clear();
            return;
        }

        if !self.utf8_buf.is_empty() {
            self.utf8_buf.push(b);
            if let Ok(s) = std::str::from_utf8(&self.utf8_buf) {
                if let Some(ch) = s.chars().next() {
                    self.put_char(ch);
                }
                self.utf8_buf.clear();
            } else if self.utf8_buf.len() >= 4 {
                self.utf8_buf.clear();
            }
            return;
        }

        match b {
            b'\r' => {
                self.cursor_col = 0;
                self.pending_wrap = false;
            }
            b'\n' => {
                self.linefeed();
            }
            0x08 => {
                self.pending_wrap = false;
                self.cursor_col = self.cursor_col.saturating_sub(1);
            }
            b'\t' => {
                let next_tab = ((self.cursor_col / 8) + 1) * 8;
                while self.cursor_col < next_tab && self.cursor_col < self.cols {
                    self.put_char(' ');
                }
            }
            0x00..=0x1F | 0x7F => {}
            0x20..=0x7E => {
                self.put_char(b as char);
            }
            _ => {
                self.utf8_buf.push(b);
            }
        }
    }

    fn is_escape_complete(&self) -> bool {
        if self.esc_buf.is_empty() {
            return false;
        }
        match self.esc_buf[0] {
            b'[' => {
                if self.esc_buf.len() < 2 {
                    return false;
                }
                let last = *self.esc_buf.last().unwrap();
                (0x40..=0x7E).contains(&last)
            }
            b']' => {
                // OSC terminated by BEL (0x07) or ST (\x1b\\)
                let last = *self.esc_buf.last().unwrap();
                last == 0x07
                    || (self.esc_buf.len() >= 2
                        && self.esc_buf[self.esc_buf.len() - 2] == 0x1B
                        && last == b'\\')
            }
            _ => true,
        }
    }

    fn handle_escape_sequence(&mut self, seq: &[u8]) {
        if seq.is_empty() {
            return;
        }
        match seq[0] {
            b'[' => {
                let body = &seq[1..seq.len() - 1];
                let final_byte = seq[seq.len() - 1];
                self.handle_csi(body, final_byte);
            }
            b'c' => {
                // Full reset
                let (r, c) = (self.rows, self.cols);
                *self = Self::new(r, c);
            }
            _ => {}
        }
    }

    fn handle_csi(&mut self, body: &[u8], final_byte: u8) {
        let body_str = String::from_utf8_lossy(body);
        if let Some(priv_str) = body_str.strip_prefix('?') {
            let num = priv_str.parse::<u16>().unwrap_or(0);
            let enable = final_byte == b'h';
            match num {
                47 | 1047 | 1049 => {
                    if enable && !self.alternate_screen {
                        self.saved_main_cursor = (self.cursor_row, self.cursor_col);
                        self.alternate_screen = true;
                        self.alt_grid = (0..self.rows)
                            .map(|_| PhysicalRow::new(self.cols))
                            .collect();
                        self.cursor_row = 0;
                        self.cursor_col = 0;
                        self.pending_wrap = false;
                    } else if !enable && self.alternate_screen {
                        self.alternate_screen = false;
                        self.cursor_row = self.saved_main_cursor.0.min(self.rows.saturating_sub(1));
                        self.cursor_col = self.saved_main_cursor.1.min(self.cols.saturating_sub(1));
                        self.pending_wrap = false;
                    }
                }
                2004 => {
                    self.bracketed_paste = enable;
                }
                25 => {
                    self.cursor_visible = enable;
                }
                _ => {}
            }
            return;
        }

        let params: Vec<u16> = if body_str.is_empty() {
            Vec::new()
        } else {
            body_str
                .split(';')
                .map(|s| s.parse::<u16>().unwrap_or(0))
                .collect()
        };

        match final_byte {
            b'm' => self.handle_sgr(&params),
            b'A' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.pending_wrap = false;
            }
            b'B' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
                self.pending_wrap = false;
            }
            b'C' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = (self.cursor_col + n).min(self.cols.saturating_sub(1));
                self.pending_wrap = false;
            }
            b'D' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = self.cursor_col.saturating_sub(n);
                self.pending_wrap = false;
            }
            b'G' => {
                let col = params.first().copied().unwrap_or(1).max(1) - 1;
                self.cursor_col = col.min(self.cols.saturating_sub(1));
                self.pending_wrap = false;
            }
            b'H' | b'f' => {
                let r = params.first().copied().unwrap_or(1).max(1) - 1;
                let c = params.get(1).copied().unwrap_or(1).max(1) - 1;
                self.cursor_row = r.min(self.rows.saturating_sub(1));
                self.cursor_col = c.min(self.cols.saturating_sub(1));
                self.pending_wrap = false;
            }
            b'J' => {
                let mode = params.first().copied().unwrap_or(0);
                let rows = self.rows as usize;
                let cols = self.cols;
                let cr = self.cursor_row as usize;
                let cc = self.cursor_col as usize;
                let grid = self.active_grid_mut();
                match mode {
                    0 => {
                        if cr < grid.len() {
                            for c in cc..grid[cr].cells.len() {
                                grid[cr].cells[c] = RenderableCell::default();
                            }
                            grid[cr].wrapped = false;
                        }
                        for r in (cr + 1)..rows.min(grid.len()) {
                            grid[r] = PhysicalRow::new(cols);
                        }
                    }
                    1 => {
                        for r in 0..cr.min(grid.len()) {
                            grid[r] = PhysicalRow::new(cols);
                        }
                        if cr < grid.len() {
                            for c in 0..=cc.min(grid[cr].cells.len().saturating_sub(1)) {
                                grid[cr].cells[c] = RenderableCell::default();
                            }
                        }
                    }
                    2 | 3 => {
                        for r in 0..rows.min(grid.len()) {
                            grid[r] = PhysicalRow::new(cols);
                        }
                        if mode == 3 && !self.alternate_screen {
                            self.scrollback.clear();
                        }
                    }
                    _ => {}
                }
            }
            b'K' => {
                let mode = params.first().copied().unwrap_or(0);
                let cr = self.cursor_row as usize;
                let cc = self.cursor_col as usize;
                let grid = self.active_grid_mut();
                if cr < grid.len() {
                    let row_len = grid[cr].cells.len();
                    match mode {
                        0 => {
                            for c in cc..row_len {
                                grid[cr].cells[c] = RenderableCell::default();
                            }
                            grid[cr].wrapped = false;
                        }
                        1 => {
                            for c in 0..=cc.min(row_len.saturating_sub(1)) {
                                grid[cr].cells[c] = RenderableCell::default();
                            }
                        }
                        2 => {
                            for c in 0..row_len {
                                grid[cr].cells[c] = RenderableCell::default();
                            }
                            grid[cr].wrapped = false;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.pen = PenState::default();
            return;
        }
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.pen = PenState::default(),
                1 => self.pen.bold = true,
                4 => self.pen.underline = true,
                7 => self.pen.inverse = true,
                22 => self.pen.bold = false,
                24 => self.pen.underline = false,
                27 => self.pen.inverse = false,
                30..=37 => self.pen.fg = ansi_idx_to_rgb((params[i] - 30) as u8),
                39 => self.pen.fg = RgbColor::DEFAULT_FG,
                40..=47 => self.pen.bg = ansi_idx_to_rgb((params[i] - 40) as u8),
                49 => self.pen.bg = RgbColor::DEFAULT_BG,
                90..=97 => self.pen.fg = ansi_idx_to_rgb((params[i] - 90 + 8) as u8),
                100..=107 => self.pen.bg = ansi_idx_to_rgb((params[i] - 100 + 8) as u8),
                38 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.pen.fg = ansi_idx_to_rgb(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        self.pen.fg = RgbColor(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                }
                48 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.pen.bg = ansi_idx_to_rgb(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        self.pen.bg = RgbColor(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn linefeed(&mut self) {
        self.pending_wrap = false;
        if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        } else if self.alternate_screen {
            if !self.alt_grid.is_empty() {
                self.alt_grid.remove(0);
                self.alt_grid.push(PhysicalRow::new(self.cols));
            }
        } else {
            if !self.grid.is_empty() {
                let top = self.grid.remove(0);
                self.scrollback.push_back(top);
                while self.scrollback.len() > SCROLLBACK_LEN {
                    self.scrollback.pop_front();
                }
                self.grid.push(PhysicalRow::new(self.cols));
            }
        }
    }

    fn put_char(&mut self, ch: char) {
        let width = char_display_width(ch);
        let is_wide = width == 2;

        if self.pending_wrap || (is_wide && self.cursor_col + 1 >= self.cols && self.cols > 1) {
            let cr = self.cursor_row as usize;
            if cr < self.active_grid().len() {
                self.active_grid_mut()[cr].wrapped = true;
            }
            self.cursor_col = 0;
            self.linefeed();
        }

        let cr = self.cursor_row as usize;
        let cc = self.cursor_col as usize;
        let cols = self.cols as usize;
        let pen = self.pen.clone();
        let grid = self.active_grid_mut();

        if cr < grid.len() && cc < cols {
            grid[cr].cells[cc] = RenderableCell {
                ch: ch.to_string(),
                fg: pen.fg,
                bg: pen.bg,
                bold: pen.bold,
                underline: pen.underline,
                inverse: pen.inverse,
                is_wide,
                is_wide_continuation: false,
            };

            if is_wide && cc + 1 < cols {
                grid[cr].cells[cc + 1] = RenderableCell {
                    ch: String::new(),
                    fg: pen.fg,
                    bg: pen.bg,
                    bold: pen.bold,
                    underline: pen.underline,
                    inverse: pen.inverse,
                    is_wide: false,
                    is_wide_continuation: true,
                };
            }
        }

        let step = if is_wide && self.cols > 1 { 2 } else { 1 };
        if self.cursor_col + step >= self.cols {
            self.cursor_col = self.cols.saturating_sub(1);
            self.pending_wrap = true;
        } else {
            self.cursor_col += step;
            self.pending_wrap = false;
        }
    }

    /// Returns a visible row at `row_idx` (`0..rows`) taking `scroll_offset` into account.
    pub fn visible_row(&self, row_idx: u16, scroll_offset: usize) -> Option<&PhysicalRow> {
        let r = row_idx as usize;
        if r >= self.rows as usize {
            return None;
        }
        if self.alternate_screen || scroll_offset == 0 {
            return self.active_grid().get(r);
        }
        let sb_len = self.scrollback.len();
        let clamped_offset = scroll_offset.min(sb_len);
        let abs_idx = (sb_len - clamped_offset) + r;
        if abs_idx < sb_len {
            self.scrollback.get(abs_idx)
        } else {
            self.grid.get(abs_idx - sb_len)
        }
    }

    /// Extracts all logical lines from `(include_scrollback)` up to the last active row on screen.
    pub fn extract_all_logical_lines(&self, include_scrollback: bool) -> Vec<Vec<RenderableCell>> {
        let mut physical_rows: Vec<(&PhysicalRow, bool)> = Vec::new();
        if include_scrollback && !self.alternate_screen {
            for r in &self.scrollback {
                physical_rows.push((r, false));
            }
        }
        let cr = self.cursor_row as usize;
        let last_active_screen_row = self.find_last_active_screen_row();
        for (idx, r) in self.grid.iter().enumerate() {
            if idx <= last_active_screen_row {
                physical_rows.push((r, idx == cr));
            }
        }

        let mut logical_lines: Vec<Vec<RenderableCell>> = Vec::new();
        let mut current_line: Vec<RenderableCell> = Vec::new();

        for (row, is_cursor_row) in physical_rows {
            for cell in &row.cells {
                if !cell.is_wide_continuation {
                    current_line.push(cell.clone());
                }
            }
            if !row.wrapped {
                let min_keep = if is_cursor_row {
                    self.cursor_col as usize
                } else {
                    0
                };
                trim_trailing_default_spaces(&mut current_line, min_keep);
                logical_lines.push(std::mem::take(&mut current_line));
            }
        }

        if !current_line.is_empty() {
            trim_trailing_default_spaces(&mut current_line, 0);
            logical_lines.push(current_line);
        }

        logical_lines
    }

    fn find_last_active_screen_row(&self) -> usize {
        let cr = self.cursor_row as usize;
        let mut last = cr;
        for (idx, row) in self.grid.iter().enumerate() {
            if row.wrapped || row.cells.iter().any(|c| !is_default_blank_cell(c)) {
                if idx > last {
                    last = idx;
                }
            }
        }
        last.min(self.grid.len().saturating_sub(1))
    }

    /// Serializes a slice of `RenderableCell` into ANSI escape sequences (`\x1b[...m` + UTF-8 text).
    pub fn serialize_cells_to_ansi(cells: &[RenderableCell]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cur_pen = PenState::default();

        for cell in cells {
            if cell.is_wide_continuation {
                continue;
            }
            if cell.fg != cur_pen.fg
                || cell.bg != cur_pen.bg
                || cell.bold != cur_pen.bold
                || cell.underline != cur_pen.underline
                || cell.inverse != cur_pen.inverse
            {
                out.extend_from_slice(b"\x1b[0");
                if cell.bold {
                    out.extend_from_slice(b";1");
                }
                if cell.underline {
                    out.extend_from_slice(b";4");
                }
                if cell.inverse {
                    out.extend_from_slice(b";7");
                }
                if cell.fg != RgbColor::DEFAULT_FG {
                    out.extend_from_slice(
                        format!(";38;2;{};{};{}", cell.fg.0, cell.fg.1, cell.fg.2).as_bytes(),
                    );
                }
                if cell.bg != RgbColor::DEFAULT_BG {
                    out.extend_from_slice(
                        format!(";48;2;{};{};{}", cell.bg.0, cell.bg.1, cell.bg.2).as_bytes(),
                    );
                }
                out.push(b'm');
                cur_pen = PenState {
                    fg: cell.fg,
                    bg: cell.bg,
                    bold: cell.bold,
                    underline: cell.underline,
                    inverse: cell.inverse,
                };
            }
            let text = if cell.ch.is_empty() { " " } else { &cell.ch };
            out.extend_from_slice(text.as_bytes());
        }

        if cur_pen.fg != RgbColor::DEFAULT_FG
            || cur_pen.bg != RgbColor::DEFAULT_BG
            || cur_pen.bold
            || cur_pen.underline
            || cur_pen.inverse
        {
            out.extend_from_slice(b"\x1b[0m");
        }
        out
    }

    /// Resizes the terminal and reflows logical lines if not in alternate screen mode (Section 9.2).
    pub fn resize(&mut self, new_rows: u16, new_cols: u16) {
        let new_rows = new_rows.max(1);
        let new_cols = new_cols.max(1);
        if new_rows == self.rows && new_cols == self.cols {
            return;
        }

        if self.alternate_screen {
            self.rows = new_rows;
            self.cols = new_cols;
            self.alt_grid.resize_with(new_rows as usize, || PhysicalRow::new(new_cols));
            for row in &mut self.alt_grid {
                row.cells.resize(new_cols as usize, RenderableCell::default());
            }
            self.cursor_row = self.cursor_row.min(new_rows - 1);
            self.cursor_col = self.cursor_col.min(new_cols - 1);
            self.generation = self.generation.wrapping_add(1);
            return;
        }

        let logical_lines = self.extract_all_logical_lines(true);
        let saved_pen = self.pen.clone();
        let saved_bracketed = self.bracketed_paste;
        let saved_cursor_vis = self.cursor_visible;

        let mut rebuilt = Self::new(new_rows, new_cols);
        rebuilt.bracketed_paste = saved_bracketed;
        rebuilt.cursor_visible = saved_cursor_vis;

        for (idx, line) in logical_lines.iter().enumerate() {
            let ansi = Self::serialize_cells_to_ansi(line);
            rebuilt.process_bytes(&ansi);
            if idx + 1 < logical_lines.len() {
                rebuilt.process_bytes(b"\r\n");
            }
        }
        rebuilt.pen = saved_pen;
        rebuilt.generation = self.generation.wrapping_add(1);
        *self = rebuilt;
    }

    /// Clears scrollback history while preserving the current visible screen (Section 9.2.4).
    pub fn clear_scrollback(&mut self) {
        if self.alternate_screen {
            return;
        }
        let live_lines = self.extract_all_logical_lines(false);
        let saved_pen = self.pen.clone();
        let saved_bracketed = self.bracketed_paste;
        let saved_cursor_vis = self.cursor_visible;

        let mut rebuilt = Self::new(self.rows, self.cols);
        rebuilt.bracketed_paste = saved_bracketed;
        rebuilt.cursor_visible = saved_cursor_vis;

        for (idx, line) in live_lines.iter().enumerate() {
            let ansi = Self::serialize_cells_to_ansi(line);
            rebuilt.process_bytes(&ansi);
            if idx + 1 < live_lines.len() {
                rebuilt.process_bytes(b"\r\n");
            }
        }
        rebuilt.pen = saved_pen;
        rebuilt.generation = self.generation.wrapping_add(1);
        *self = rebuilt;
    }

    /// Extracts text between two absolute `(row, col)` coordinates (including scrollback).
    pub fn extract_full_selected_text(
        &self,
        start: (usize, usize),
        end: (usize, usize),
    ) -> String {
        let (s, e) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let sb_len = if self.alternate_screen {
            0
        } else {
            self.scrollback.len()
        };
        let total_rows = sb_len + self.active_grid().len();
        let mut lines = Vec::new();

        for abs_r in s.0..=e.0.min(total_rows.saturating_sub(1)) {
            let row = if abs_r < sb_len {
                &self.scrollback[abs_r]
            } else {
                &self.active_grid()[abs_r - sb_len]
            };
            let c_start = if abs_r == s.0 { s.1 } else { 0 };
            let c_end = if abs_r == e.0 {
                e.1.min(row.cells.len().saturating_sub(1))
            } else {
                row.cells.len().saturating_sub(1)
            };
            if c_start <= c_end && c_start < row.cells.len() {
                let mut line_str = String::new();
                for c in c_start..=c_end.min(row.cells.len() - 1) {
                    let cell = &row.cells[c];
                    if !cell.is_wide_continuation {
                        line_str.push_str(if cell.ch.is_empty() { " " } else { &cell.ch });
                    }
                }
                lines.push(line_str.trim_end().to_string());
            }
        }

        lines.join("\n")
    }
}

fn is_default_blank_cell(cell: &RenderableCell) -> bool {
    (cell.ch == " " || cell.ch.is_empty())
        && cell.fg == RgbColor::DEFAULT_FG
        && cell.bg == RgbColor::DEFAULT_BG
        && !cell.bold
        && !cell.underline
        && !cell.inverse
        && !cell.is_wide
        && !cell.is_wide_continuation
}

fn trim_trailing_default_spaces(cells: &mut Vec<RenderableCell>, min_keep: usize) {
    while cells.len() > min_keep {
        if let Some(last) = cells.last() {
            if is_default_blank_cell(last) {
                cells.pop();
            } else {
                break;
            }
        } else {
            break;
        }
    }
}

pub fn char_display_width(ch: char) -> u16 {
    let u = ch as u32;
    if (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE10..=0xFE19).contains(&u)
        || (0xFE30..=0xFE6F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x1F300..=0x1F9FF).contains(&u)
        || (0x20000..=0x3FFFD).contains(&u)
    {
        2
    } else {
        1
    }
}

#[cfg(test)]
#[path = "test/vt100_adapter_tests.rs"]
mod vt100_adapter_tests;
