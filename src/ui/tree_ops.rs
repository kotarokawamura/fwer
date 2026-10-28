// Copyright (C) [2026/10/28] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::collections::HashMap;

use super::state::TileId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneMeta {
    pub id: TileId,
    pub title: String,
    pub command: Option<String>,
    pub broadcast_group: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeNode {
    Pane(TileId),
    Horizontal(Vec<TreeNode>),
    Vertical(Vec<TreeNode>),
    Tabs {
        children: Vec<TreeNode>,
        active_idx: usize,
    },
}

#[derive(Debug, Clone)]
pub struct WorkspaceTree {
    next_id: u64,
    pub root: Option<TreeNode>,
    pub panes: HashMap<TileId, PaneMeta>,
    pub active_pane: Option<TileId>,
}

impl Default for WorkspaceTree {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceTree {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            root: None,
            panes: HashMap::new(),
            active_pane: None,
        }
    }

    pub fn allocate_pane(&mut self, title: impl Into<String>, command: Option<String>) -> TileId {
        let id = TileId(self.next_id);
        self.next_id += 1;
        self.panes.insert(
            id,
            PaneMeta {
                id,
                title: title.into(),
                command,
                broadcast_group: 0,
            },
        );
        if self.active_pane.is_none() {
            self.active_pane = Some(id);
        }
        id
    }

    pub fn with_initial_pane(title: impl Into<String>, command: Option<String>) -> (Self, TileId) {
        let mut tree = Self::new();
        let id = tree.allocate_pane(title, command);
        tree.root = Some(TreeNode::Pane(id));
        tree.active_pane = Some(id);
        (tree, id)
    }

    pub fn ordered_pane_ids(&self) -> Vec<TileId> {
        let mut out = Vec::new();
        if let Some(ref root) = self.root {
            collect_pane_ids(root, &mut out);
        }
        out
    }

    pub fn set_active_pane(&mut self, target: TileId) {
        if !self.panes.contains_key(&target) {
            return;
        }
        self.active_pane = Some(target);
        if let Some(ref mut root) = self.root {
            activate_tabs_for_pane(root, target);
        }
    }

    /// Splits `target` horizontally (`horizontal = true` -> side-by-side) or vertically (`horizontal = false` -> top-bottom).
    pub fn split_pane(
        &mut self,
        target: TileId,
        horizontal: bool,
        title: impl Into<String>,
        command: Option<String>,
    ) -> Option<TileId> {
        if !self.panes.contains_key(&target) {
            return None;
        }
        let new_id = self.allocate_pane(title, command);
        if let Some(ref mut root) = self.root {
            replace_with_split(root, target, new_id, horizontal);
        }
        self.set_active_pane(new_id);
        Some(new_id)
    }

    /// Adds a new tab alongside `target` (converting `target` into a `Tabs` container if not already in one).
    pub fn add_tab(
        &mut self,
        target: TileId,
        title: impl Into<String>,
        command: Option<String>,
    ) -> Option<TileId> {
        if !self.panes.contains_key(&target) {
            return None;
        }
        let new_id = self.allocate_pane(title, command);
        if let Some(ref mut root) = self.root {
            insert_tab_at(root, target, new_id);
        }
        self.set_active_pane(new_id);
        Some(new_id)
    }

    /// Removes `target` from the tree. Returns `true` if the workspace is now completely empty.
    pub fn close_pane(&mut self, target: TileId) -> bool {
        let ordered_before = self.ordered_pane_ids();
        let target_pos = ordered_before.iter().position(|&id| id == target);

        self.panes.remove(&target);
        if let Some(root) = self.root.take() {
            self.root = remove_pane_from_node(root, target);
        }
        self.cleanup_singleton_broadcast_groups();

        let remaining = self.ordered_pane_ids();
        if remaining.is_empty() {
            self.active_pane = None;
            return true;
        }

        if self.active_pane == Some(target) || !self.active_pane.is_some_and(|a| self.panes.contains_key(&a)) {
            let next_focus = target_pos
                .map(|idx| remaining[idx.min(remaining.len() - 1)])
                .unwrap_or(remaining[0]);
            self.set_active_pane(next_focus);
        }
        false
    }

    /// Swaps the positions of two panes `a` and `b` in the layout tree.
    pub fn swap_panes(&mut self, a: TileId, b: TileId) -> bool {
        if a == b || !self.panes.contains_key(&a) || !self.panes.contains_key(&b) {
            return false;
        }
        if let Some(ref mut root) = self.root {
            swap_in_node(root, a, b);
            if let Some(active) = self.active_pane {
                activate_tabs_for_pane(root, active);
            }
            return true;
        }
        false
    }

    pub fn focus_next_pane(&mut self) -> Option<TileId> {
        let ids = self.ordered_pane_ids();
        if ids.is_empty() {
            return None;
        }
        let next = match self.active_pane.and_then(|cur| ids.iter().position(|&x| x == cur)) {
            Some(idx) => ids[(idx + 1) % ids.len()],
            None => ids[0],
        };
        self.set_active_pane(next);
        Some(next)
    }

    pub fn focus_prev_pane(&mut self) -> Option<TileId> {
        let ids = self.ordered_pane_ids();
        if ids.is_empty() {
            return None;
        }
        let prev = match self.active_pane.and_then(|cur| ids.iter().position(|&x| x == cur)) {
            Some(0) => ids[ids.len() - 1],
            Some(idx) => ids[idx - 1],
            None => ids[0],
        };
        self.set_active_pane(prev);
        Some(prev)
    }

    /// Determines the pane currently located at the top-right corner of the workspace (Section 10.4).
    pub fn top_right_pane_id(&self) -> Option<TileId> {
        self.root.as_ref().and_then(find_top_right_pane)
    }

    /// Links `source` and `target` into the same broadcast group (Section 10.3).
    pub fn link_broadcast_panes(&mut self, source: TileId, target: TileId) -> usize {
        if source == target
            || !self.panes.contains_key(&source)
            || !self.panes.contains_key(&target)
        {
            return 0;
        }
        let src_grp = self.panes.get(&source).map(|p| p.broadcast_group).unwrap_or(0);
        let dst_grp = self.panes.get(&target).map(|p| p.broadcast_group).unwrap_or(0);

        let group_id = if src_grp > 0 {
            src_grp
        } else if dst_grp > 0 {
            dst_grp
        } else {
            self.next_available_broadcast_group()
        };

        if let Some(p) = self.panes.get_mut(&source) {
            p.broadcast_group = group_id;
        }
        if let Some(p) = self.panes.get_mut(&target) {
            p.broadcast_group = group_id;
        }
        group_id
    }

    pub fn leave_broadcast_group(&mut self, target: TileId) {
        if let Some(p) = self.panes.get_mut(&target) {
            p.broadcast_group = 0;
        }
        self.cleanup_singleton_broadcast_groups();
    }

    pub fn cleanup_singleton_broadcast_groups(&mut self) {
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for p in self.panes.values() {
            if p.broadcast_group > 0 {
                *counts.entry(p.broadcast_group).or_insert(0) += 1;
            }
        }
        for p in self.panes.values_mut() {
            if p.broadcast_group > 0 && counts.get(&p.broadcast_group).copied().unwrap_or(0) <= 1 {
                p.broadcast_group = 0;
            }
        }
    }

    fn next_available_broadcast_group(&self) -> usize {
        let mut g = 1usize;
        while self.panes.values().any(|p| p.broadcast_group == g) {
            g += 1;
        }
        g
    }

    /// Returns all other pane IDs in the same broadcast group as `source`.
    pub fn broadcast_peers(&self, source: TileId) -> Vec<TileId> {
        let Some(meta) = self.panes.get(&source) else {
            return Vec::new();
        };
        if meta.broadcast_group == 0 {
            return Vec::new();
        }
        let grp = meta.broadcast_group;
        let mut peers: Vec<TileId> = self
            .ordered_pane_ids()
            .into_iter()
            .filter(|&id| {
                id != source
                    && self
                        .panes
                        .get(&id)
                        .is_some_and(|p| p.broadcast_group == grp)
            })
            .collect();
        peers.dedup();
        peers
    }
}

fn collect_pane_ids(node: &TreeNode, out: &mut Vec<TileId>) {
    match node {
        TreeNode::Pane(id) => out.push(*id),
        TreeNode::Horizontal(children) | TreeNode::Vertical(children) => {
            for c in children {
                collect_pane_ids(c, out);
            }
        }
        TreeNode::Tabs { children, .. } => {
            for c in children {
                collect_pane_ids(c, out);
            }
        }
    }
}

fn activate_tabs_for_pane(node: &mut TreeNode, target: TileId) -> bool {
    match node {
        TreeNode::Pane(id) => *id == target,
        TreeNode::Horizontal(children) | TreeNode::Vertical(children) => {
            let mut found = false;
            for c in children {
                if activate_tabs_for_pane(c, target) {
                    found = true;
                }
            }
            found
        }
        TreeNode::Tabs {
            children,
            active_idx,
        } => {
            for (idx, c) in children.iter_mut().enumerate() {
                if activate_tabs_for_pane(c, target) {
                    *active_idx = idx;
                    return true;
                }
            }
            false
        }
    }
}

fn replace_with_split(node: &mut TreeNode, target: TileId, new_id: TileId, horizontal: bool) -> bool {
    match node {
        TreeNode::Pane(id) if *id == target => {
            let pair = vec![TreeNode::Pane(target), TreeNode::Pane(new_id)];
            *node = if horizontal {
                TreeNode::Horizontal(pair)
            } else {
                TreeNode::Vertical(pair)
            };
            true
        }
        TreeNode::Pane(_) => false,
        TreeNode::Horizontal(children) => {
            if horizontal {
                if let Some(pos) = children
                    .iter()
                    .position(|c| matches!(c, TreeNode::Pane(id) if *id == target))
                {
                    children.insert(pos + 1, TreeNode::Pane(new_id));
                    return true;
                }
            }
            for c in children {
                if replace_with_split(c, target, new_id, horizontal) {
                    return true;
                }
            }
            false
        }
        TreeNode::Vertical(children) => {
            if !horizontal {
                if let Some(pos) = children
                    .iter()
                    .position(|c| matches!(c, TreeNode::Pane(id) if *id == target))
                {
                    children.insert(pos + 1, TreeNode::Pane(new_id));
                    return true;
                }
            }
            for c in children {
                if replace_with_split(c, target, new_id, horizontal) {
                    return true;
                }
            }
            false
        }
        TreeNode::Tabs { children, .. } => {
            for c in children {
                if replace_with_split(c, target, new_id, horizontal) {
                    return true;
                }
            }
            false
        }
    }
}

fn insert_tab_at(node: &mut TreeNode, target: TileId, new_id: TileId) -> bool {
    match node {
        TreeNode::Pane(id) if *id == target => {
            *node = TreeNode::Tabs {
                children: vec![TreeNode::Pane(target), TreeNode::Pane(new_id)],
                active_idx: 1,
            };
            true
        }
        TreeNode::Pane(_) => false,
        TreeNode::Tabs {
            children,
            active_idx,
        } => {
            if let Some(pos) = children
                .iter()
                .position(|c| matches!(c, TreeNode::Pane(id) if *id == target))
            {
                children.insert(pos + 1, TreeNode::Pane(new_id));
                *active_idx = pos + 1;
                return true;
            }
            for c in children {
                if insert_tab_at(c, target, new_id) {
                    return true;
                }
            }
            false
        }
        TreeNode::Horizontal(children) | TreeNode::Vertical(children) => {
            for c in children {
                if insert_tab_at(c, target, new_id) {
                    return true;
                }
            }
            false
        }
    }
}

fn remove_pane_from_node(node: TreeNode, target: TileId) -> Option<TreeNode> {
    match node {
        TreeNode::Pane(id) => {
            if id == target {
                None
            } else {
                Some(TreeNode::Pane(id))
            }
        }
        TreeNode::Horizontal(children) => {
            let mut kept: Vec<TreeNode> = children
                .into_iter()
                .filter_map(|c| remove_pane_from_node(c, target))
                .collect();
            match kept.len() {
                0 => None,
                1 => Some(kept.remove(0)),
                _ => Some(TreeNode::Horizontal(kept)),
            }
        }
        TreeNode::Vertical(children) => {
            let mut kept: Vec<TreeNode> = children
                .into_iter()
                .filter_map(|c| remove_pane_from_node(c, target))
                .collect();
            match kept.len() {
                0 => None,
                1 => Some(kept.remove(0)),
                _ => Some(TreeNode::Vertical(kept)),
            }
        }
        TreeNode::Tabs {
            children,
            active_idx,
        } => {
            let mut kept: Vec<TreeNode> = children
                .into_iter()
                .filter_map(|c| remove_pane_from_node(c, target))
                .collect();
            match kept.len() {
                0 => None,
                1 => Some(kept.remove(0)),
                len => Some(TreeNode::Tabs {
                    children: kept,
                    active_idx: active_idx.min(len - 1),
                }),
            }
        }
    }
}

fn swap_in_node(node: &mut TreeNode, a: TileId, b: TileId) {
    match node {
        TreeNode::Pane(id) => {
            if *id == a {
                *id = b;
            } else if *id == b {
                *id = a;
            }
        }
        TreeNode::Horizontal(children)
        | TreeNode::Vertical(children)
        | TreeNode::Tabs { children, .. } => {
            for c in children {
                swap_in_node(c, a, b);
            }
        }
    }
}

fn find_top_right_pane(node: &TreeNode) -> Option<TileId> {
    match node {
        TreeNode::Pane(id) => Some(*id),
        TreeNode::Horizontal(children) => children.last().and_then(find_top_right_pane),
        TreeNode::Vertical(children) => children.first().and_then(find_top_right_pane),
        TreeNode::Tabs {
            children,
            active_idx,
        } => children
            .get(*active_idx)
            .or_else(|| children.first())
            .and_then(find_top_right_pane),
    }
}
