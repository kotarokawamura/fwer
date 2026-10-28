use super::*;

#[test]
fn test_layout_node_roundtrip_and_build_tree() {
    let (mut tree, p1) = WorkspaceTree::with_initial_pane("Shell 1", None);
    let p2 = tree
        .split_pane(p1, true, "Shell 2", Some("htop".to_string()))
        .unwrap();
    let _p3 = tree.add_tab(p2, "Shell 3", None).unwrap();

    let preset = LayoutNode::from_tree(&tree).expect("from_tree");
    let json = serde_json::to_string_pretty(&preset).expect("serialize");
    let decoded: LayoutNode = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(preset, decoded);

    let rebuilt = decoded.build_tree();
    let ids = rebuilt.ordered_pane_ids();
    assert_eq!(ids.len(), 3);
    assert_eq!(rebuilt.active_pane, Some(ids[0]));
}
