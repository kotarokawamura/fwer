// Copyright (C) [2026/10/19] kotaro kawamura
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::theme::config_path;
use crate::ui::tree_ops::{TreeNode, WorkspaceTree};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LayoutNode {
    Pane {
        title: String,
        command: Option<String>,
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

pub fn layout_file_path() -> PathBuf {
    let cfg = config_path();
    if let Some(parent) = cfg.parent() {
        parent.join("layout.json")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config/termin/layout.json")
    }
}

impl LayoutNode {
    pub fn from_tree(tree: &WorkspaceTree) -> Option<Self> {
        let root = tree.root.as_ref()?;
        Some(Self::from_tree_node(root, tree))
    }

    fn from_tree_node(node: &TreeNode, tree: &WorkspaceTree) -> Self {
        match node {
            TreeNode::Pane(id) => {
                let (title, command) = tree
                    .panes
                    .get(id)
                    .map(|p| (p.title.clone(), p.command.clone()))
                    .unwrap_or_else(|| ("/bin/bash".to_string(), None));
                LayoutNode::Pane { title, command }
            }
            TreeNode::Horizontal(children) => LayoutNode::HorizontalSplit {
                children: children
                    .iter()
                    .map(|c| Self::from_tree_node(c, tree))
                    .collect(),
            },
            TreeNode::Vertical(children) => LayoutNode::VerticalSplit {
                children: children
                    .iter()
                    .map(|c| Self::from_tree_node(c, tree))
                    .collect(),
            },
            TreeNode::Tabs { children, .. } => LayoutNode::Tabs {
                children: children
                    .iter()
                    .map(|c| Self::from_tree_node(c, tree))
                    .collect(),
            },
        }
    }

    pub fn build_tree(&self) -> WorkspaceTree {
        let mut tree = WorkspaceTree::new();
        let root = self.build_node(&mut tree);
        tree.root = Some(root);
        if let Some(&first_id) = tree.ordered_pane_ids().first() {
            tree.set_active_pane(first_id);
        }
        tree
    }

    fn build_node(&self, tree: &mut WorkspaceTree) -> TreeNode {
        match self {
            LayoutNode::Pane { title, command } => {
                let id = tree.allocate_pane(title.clone(), command.clone());
                TreeNode::Pane(id)
            }
            LayoutNode::HorizontalSplit { children } => {
                if children.is_empty() {
                    let id = tree.allocate_pane("/bin/bash", None);
                    TreeNode::Pane(id)
                } else {
                    TreeNode::Horizontal(children.iter().map(|c| c.build_node(tree)).collect())
                }
            }
            LayoutNode::VerticalSplit { children } => {
                if children.is_empty() {
                    let id = tree.allocate_pane("/bin/bash", None);
                    TreeNode::Pane(id)
                } else {
                    TreeNode::Vertical(children.iter().map(|c| c.build_node(tree)).collect())
                }
            }
            LayoutNode::Tabs { children } => {
                if children.is_empty() {
                    let id = tree.allocate_pane("/bin/bash", None);
                    TreeNode::Pane(id)
                } else {
                    TreeNode::Tabs {
                        children: children.iter().map(|c| c.build_node(tree)).collect(),
                        active_idx: 0,
                    }
                }
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
