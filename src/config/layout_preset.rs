// Copyright (C) [2026/10/19] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::theme::config_path;
use crate::ui::state::TileId;
use crate::ui::tree_ops::{TreeNode, WorkspaceTree};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LayoutNode {
    Pane {
        title: String,
        #[serde(default)]
        command: Option<String>,
        #[serde(default)]
        working_dir: Option<String>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        font_size: Option<f32>,
        #[serde(default)]
        show_scrollbar: Option<bool>,
    },
    HorizontalSplit {
        children: Vec<LayoutNode>,
    },
    VerticalSplit {
        children: Vec<LayoutNode>,
    },
    Tabs {
        children: Vec<LayoutNode>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanePresetConfig {
    pub title: String,
    pub command: Option<String>,
    pub working_dir: Option<String>,
    pub opacity: Option<f32>,
    pub font_size: Option<f32>,
    pub show_scrollbar: Option<bool>,
}

impl Default for PanePresetConfig {
    fn default() -> Self {
        Self {
            title: "/bin/bash".to_string(),
            command: None,
            working_dir: None,
            opacity: None,
            font_size: None,
            show_scrollbar: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WorkspacePreset {
    pub name: String,
    pub run_on_startup: bool,
    pub window_opacity: Option<f32>,
    pub window_font_size: Option<f32>,
    pub ui_font_size: Option<f32>,
    pub root: LayoutNode,
}

impl Default for WorkspacePreset {
    fn default() -> Self {
        Self {
            name: "Preset 1".to_string(),
            run_on_startup: false,
            window_opacity: None,
            window_font_size: None,
            ui_font_size: None,
            root: LayoutNode::Pane {
                title: "/bin/bash".to_string(),
                command: None,
                working_dir: None,
                opacity: None,
                font_size: None,
                show_scrollbar: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct PresetStore {
    pub presets: Vec<WorkspacePreset>,
}

pub fn layout_file_path() -> PathBuf {
    let cfg = config_path();
    if let Some(parent) = cfg.parent() {
        parent.join("layout.json")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config/termin/layout.json")
    }
}

pub fn presets_file_path() -> PathBuf {
    let cfg = config_path();
    if let Some(parent) = cfg.parent() {
        parent.join("presets.json")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config/termin/presets.json")
    }
}

pub fn expand_tilde_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return Some(PathBuf::from(home));
        }
    } else if let Some(rest) = trimmed.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return Some(PathBuf::from(home).join(rest));
    }
    Some(PathBuf::from(trimmed))
}

impl PresetStore {
    pub fn load() -> Self {
        let path = presets_file_path();
        if let Ok(raw) = fs::read_to_string(&path)
            && let Ok(store) = serde_json::from_str::<PresetStore>(&raw)
        {
            return store;
        }
        Self::default()
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let path = presets_file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(path)
    }

    pub fn startup_preset(&self) -> Option<&WorkspacePreset> {
        self.presets.iter().find(|p| p.run_on_startup)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&WorkspacePreset> {
        self.presets.iter().find(|p| p.name == name)
    }

    pub fn upsert(&mut self, preset: WorkspacePreset) {
        if let Some(existing) = self.presets.iter_mut().find(|p| p.name == preset.name) {
            *existing = preset;
        } else {
            self.presets.push(preset);
        }
    }

    pub fn remove_by_name(&mut self, name: &str) {
        self.presets.retain(|p| p.name != name);
    }

    pub fn set_startup_preset(&mut self, name: Option<&str>) {
        for p in &mut self.presets {
            p.run_on_startup = name.is_some_and(|n| p.name == n);
        }
    }
}

impl LayoutNode {
    pub fn from_tree(tree: &WorkspaceTree) -> Option<Self> {
        Self::from_tree_with_configs(tree, |id| {
            let (title, command) = tree
                .panes
                .get(&id)
                .map(|p| (p.title.clone(), p.command.clone()))
                .unwrap_or_else(|| ("/bin/bash".to_string(), None));
            PanePresetConfig {
                title,
                command,
                working_dir: None,
                opacity: None,
                font_size: None,
                show_scrollbar: None,
            }
        })
    }

    pub fn from_tree_with_configs<F>(tree: &WorkspaceTree, mut resolver: F) -> Option<Self>
    where
        F: FnMut(TileId) -> PanePresetConfig,
    {
        let root = tree.root.as_ref()?;
        Some(Self::from_tree_node(root, &mut resolver))
    }

    fn from_tree_node<F>(node: &TreeNode, resolver: &mut F) -> Self
    where
        F: FnMut(TileId) -> PanePresetConfig,
    {
        match node {
            TreeNode::Pane(id) => {
                let cfg = resolver(*id);
                LayoutNode::Pane {
                    title: cfg.title,
                    command: cfg.command,
                    working_dir: cfg.working_dir,
                    opacity: cfg.opacity,
                    font_size: cfg.font_size,
                    show_scrollbar: cfg.show_scrollbar,
                }
            }
            TreeNode::Horizontal(children) => LayoutNode::HorizontalSplit {
                children: children
                    .iter()
                    .map(|c| Self::from_tree_node(c, resolver))
                    .collect(),
            },
            TreeNode::Vertical(children) => LayoutNode::VerticalSplit {
                children: children
                    .iter()
                    .map(|c| Self::from_tree_node(c, resolver))
                    .collect(),
            },
            TreeNode::Tabs { children, .. } => LayoutNode::Tabs {
                children: children
                    .iter()
                    .map(|c| Self::from_tree_node(c, resolver))
                    .collect(),
            },
        }
    }

    pub fn pane_count(&self) -> usize {
        self.pane_configs().len()
    }

    pub fn build_tree(&self) -> WorkspaceTree {
        self.build_tree_with_configs().0
    }

    pub fn build_tree_starting_at(&self, start_id: u64) -> WorkspaceTree {
        self.build_tree_with_configs_starting_at(start_id).0
    }

    pub fn build_tree_with_configs(&self) -> (WorkspaceTree, HashMap<TileId, PanePresetConfig>) {
        self.build_tree_with_configs_starting_at(1)
    }

    pub fn build_tree_with_configs_starting_at(
        &self,
        start_id: u64,
    ) -> (WorkspaceTree, HashMap<TileId, PanePresetConfig>) {
        let mut tree = WorkspaceTree::with_start_id(start_id);
        let mut configs = HashMap::new();
        let root = self.build_node(&mut tree, &mut configs);
        tree.root = Some(root);
        if let Some(&first_id) = tree.ordered_pane_ids().first() {
            tree.set_active_pane(first_id);
        }
        (tree, configs)
    }

    fn build_node(
        &self,
        tree: &mut WorkspaceTree,
        configs: &mut HashMap<TileId, PanePresetConfig>,
    ) -> TreeNode {
        match self {
            LayoutNode::Pane {
                title,
                command,
                working_dir,
                opacity,
                font_size,
                show_scrollbar,
            } => {
                let id = tree.allocate_pane(title.clone(), command.clone());
                configs.insert(
                    id,
                    PanePresetConfig {
                        title: title.clone(),
                        command: command.clone(),
                        working_dir: working_dir.clone(),
                        opacity: *opacity,
                        font_size: *font_size,
                        show_scrollbar: *show_scrollbar,
                    },
                );
                TreeNode::Pane(id)
            }
            LayoutNode::HorizontalSplit { children } => {
                if children.is_empty() {
                    let id = tree.allocate_pane("/bin/bash", None);
                    configs.insert(id, PanePresetConfig::default());
                    TreeNode::Pane(id)
                } else {
                    TreeNode::Horizontal(
                        children
                            .iter()
                            .map(|c| c.build_node(tree, configs))
                            .collect(),
                    )
                }
            }
            LayoutNode::VerticalSplit { children } => {
                if children.is_empty() {
                    let id = tree.allocate_pane("/bin/bash", None);
                    configs.insert(id, PanePresetConfig::default());
                    TreeNode::Pane(id)
                } else {
                    TreeNode::Vertical(
                        children
                            .iter()
                            .map(|c| c.build_node(tree, configs))
                            .collect(),
                    )
                }
            }
            LayoutNode::Tabs { children } => {
                if children.is_empty() {
                    let id = tree.allocate_pane("/bin/bash", None);
                    configs.insert(id, PanePresetConfig::default());
                    TreeNode::Pane(id)
                } else {
                    TreeNode::Tabs {
                        children: children
                            .iter()
                            .map(|c| c.build_node(tree, configs))
                            .collect(),
                        active_idx: 0,
                    }
                }
            }
        }
    }

    pub fn pane_configs(&self) -> Vec<PanePresetConfig> {
        let mut out = Vec::new();
        self.collect_pane_configs(&mut out);
        out
    }

    fn collect_pane_configs(&self, out: &mut Vec<PanePresetConfig>) {
        match self {
            LayoutNode::Pane {
                title,
                command,
                working_dir,
                opacity,
                font_size,
                show_scrollbar,
            } => {
                out.push(PanePresetConfig {
                    title: title.clone(),
                    command: command.clone(),
                    working_dir: working_dir.clone(),
                    opacity: *opacity,
                    font_size: *font_size,
                    show_scrollbar: *show_scrollbar,
                });
            }
            LayoutNode::HorizontalSplit { children }
            | LayoutNode::VerticalSplit { children }
            | LayoutNode::Tabs { children } => {
                for c in children {
                    c.collect_pane_configs(out);
                }
            }
        }
    }

    pub fn update_pane_at<F>(&mut self, target_idx: usize, mut updater: F) -> bool
    where
        F: FnMut(&mut PanePresetConfig),
    {
        let mut cur_idx = 0;
        self.update_pane_recursive(target_idx, &mut cur_idx, &mut updater)
    }

    fn update_pane_recursive<F>(
        &mut self,
        target_idx: usize,
        cur_idx: &mut usize,
        updater: &mut F,
    ) -> bool
    where
        F: FnMut(&mut PanePresetConfig),
    {
        match self {
            LayoutNode::Pane {
                title,
                command,
                working_dir,
                opacity,
                font_size,
                show_scrollbar,
            } => {
                if *cur_idx == target_idx {
                    let mut cfg = PanePresetConfig {
                        title: title.clone(),
                        command: command.clone(),
                        working_dir: working_dir.clone(),
                        opacity: *opacity,
                        font_size: *font_size,
                        show_scrollbar: *show_scrollbar,
                    };
                    updater(&mut cfg);
                    *title = cfg.title;
                    *command = cfg.command;
                    *working_dir = cfg.working_dir;
                    *opacity = cfg.opacity;
                    *font_size = cfg.font_size;
                    *show_scrollbar = cfg.show_scrollbar;
                    return true;
                }
                *cur_idx += 1;
                false
            }
            LayoutNode::HorizontalSplit { children }
            | LayoutNode::VerticalSplit { children }
            | LayoutNode::Tabs { children } => {
                for c in children {
                    if c.update_pane_recursive(target_idx, cur_idx, updater) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn split_pane_at(&mut self, target_idx: usize, horizontal: bool) -> bool {
        let mut cur_idx = 0;
        self.split_pane_recursive(target_idx, &mut cur_idx, horizontal)
    }

    fn split_pane_recursive(
        &mut self,
        target_idx: usize,
        cur_idx: &mut usize,
        horizontal: bool,
    ) -> bool {
        match self {
            LayoutNode::Pane {
                working_dir,
                opacity,
                font_size,
                ..
            } => {
                if *cur_idx == target_idx {
                    let new_pane = LayoutNode::Pane {
                        title: "/bin/bash".to_string(),
                        command: None,
                        working_dir: working_dir.clone(),
                        opacity: *opacity,
                        font_size: *font_size,
                        show_scrollbar: None,
                    };
                    let existing = self.clone();
                    *self = if horizontal {
                        LayoutNode::HorizontalSplit {
                            children: vec![existing, new_pane],
                        }
                    } else {
                        LayoutNode::VerticalSplit {
                            children: vec![existing, new_pane],
                        }
                    };
                    return true;
                }
                *cur_idx += 1;
                false
            }
            LayoutNode::HorizontalSplit { children }
            | LayoutNode::VerticalSplit { children }
            | LayoutNode::Tabs { children } => {
                for c in children {
                    if c.split_pane_recursive(target_idx, cur_idx, horizontal) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn add_tab_at(&mut self, target_idx: usize) -> bool {
        let mut cur_idx = 0;
        self.add_tab_recursive(target_idx, &mut cur_idx)
    }

    fn add_tab_recursive(&mut self, target_idx: usize, cur_idx: &mut usize) -> bool {
        match self {
            LayoutNode::Pane {
                working_dir,
                opacity,
                font_size,
                ..
            } => {
                if *cur_idx == target_idx {
                    let new_pane = LayoutNode::Pane {
                        title: "/bin/bash".to_string(),
                        command: None,
                        working_dir: working_dir.clone(),
                        opacity: *opacity,
                        font_size: *font_size,
                        show_scrollbar: None,
                    };
                    let existing = self.clone();
                    *self = LayoutNode::Tabs {
                        children: vec![existing, new_pane],
                    };
                    return true;
                }
                *cur_idx += 1;
                false
            }
            LayoutNode::HorizontalSplit { children }
            | LayoutNode::VerticalSplit { children }
            | LayoutNode::Tabs { children } => {
                for c in children {
                    if c.add_tab_recursive(target_idx, cur_idx) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn remove_pane_at(&mut self, target_idx: usize) -> bool {
        if self.pane_configs().len() <= 1 {
            return false;
        }
        let mut cur_idx = 0;
        if let Some(updated) = self.clone().remove_pane_recursive(target_idx, &mut cur_idx) {
            *self = updated;
            true
        } else {
            false
        }
    }

    fn remove_pane_recursive(self, target_idx: usize, cur_idx: &mut usize) -> Option<LayoutNode> {
        match self {
            LayoutNode::Pane { .. } => {
                let idx = *cur_idx;
                *cur_idx += 1;
                if idx == target_idx {
                    None
                } else {
                    Some(self)
                }
            }
            LayoutNode::HorizontalSplit { children } => {
                let mut kept: Vec<LayoutNode> = children
                    .into_iter()
                    .filter_map(|c| c.remove_pane_recursive(target_idx, cur_idx))
                    .collect();
                match kept.len() {
                    0 => None,
                    1 => Some(kept.remove(0)),
                    _ => Some(LayoutNode::HorizontalSplit { children: kept }),
                }
            }
            LayoutNode::VerticalSplit { children } => {
                let mut kept: Vec<LayoutNode> = children
                    .into_iter()
                    .filter_map(|c| c.remove_pane_recursive(target_idx, cur_idx))
                    .collect();
                match kept.len() {
                    0 => None,
                    1 => Some(kept.remove(0)),
                    _ => Some(LayoutNode::VerticalSplit { children: kept }),
                }
            }
            LayoutNode::Tabs { children } => {
                let mut kept: Vec<LayoutNode> = children
                    .into_iter()
                    .filter_map(|c| c.remove_pane_recursive(target_idx, cur_idx))
                    .collect();
                match kept.len() {
                    0 => None,
                    1 => Some(kept.remove(0)),
                    _ => Some(LayoutNode::Tabs { children: kept }),
                }
            }
        }
    }

    pub fn describe_structure(&self) -> String {
        let mut cur_idx = 1;
        self.describe_recursive(&mut cur_idx)
    }

    fn describe_recursive(&self, cur_idx: &mut usize) -> String {
        match self {
            LayoutNode::Pane { title, .. } => {
                let s = format!("#{} ({})", *cur_idx, title);
                *cur_idx += 1;
                s
            }
            LayoutNode::HorizontalSplit { children } => {
                let inner: Vec<String> = children
                    .iter()
                    .map(|c| c.describe_recursive(cur_idx))
                    .collect();
                format!("H[{}]", inner.join(" | "))
            }
            LayoutNode::VerticalSplit { children } => {
                let inner: Vec<String> = children
                    .iter()
                    .map(|c| c.describe_recursive(cur_idx))
                    .collect();
                format!("V[{}]", inner.join(" / "))
            }
            LayoutNode::Tabs { children } => {
                let inner: Vec<String> = children
                    .iter()
                    .map(|c| c.describe_recursive(cur_idx))
                    .collect();
                format!("Tabs[{}]", inner.join(", "))
            }
        }
    }

    pub fn save_to_disk(&self) -> Result<PathBuf, String> {
        let path = layout_file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(path)
    }

    pub fn load_from_disk() -> Result<Self, String> {
        let path = layout_file_path();
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "test/layout_preset_tests.rs"]
mod layout_preset_tests;
