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

#[test]
fn test_workspace_preset_store_and_node_operations() {
    let mut store = PresetStore::default();
    let mut p1 = WorkspacePreset {
        name: "Dev Set".to_string(),
        run_on_startup: true,
        window_opacity: Some(0.9),
        window_font_size: Some(15.5),
        ui_font_size: Some(15.0),
        root: LayoutNode::Pane {
            title: "Main".to_string(),
            command: Some("nvim".to_string()),
            working_dir: Some("~/projects/app".to_string()),
            opacity: Some(0.85),
            font_size: Some(16.0),
        },
    };

    assert!(p1.root.split_pane_at(0, true));
    assert_eq!(p1.root.pane_count(), 2);
    assert!(p1.root.add_tab_at(1));
    assert_eq!(p1.root.pane_count(), 3);

    p1.root.update_pane_at(2, |cfg| {
        cfg.title = "Logs".to_string();
        cfg.command = Some("tail -f app.log".to_string());
        cfg.opacity = Some(0.75);
        cfg.font_size = Some(14.5);
    });

    let (tree1, cfgs) = p1.root.build_tree_with_configs();
    assert_eq!(cfgs.len(), 3);
    let next_start = tree1.next_id_counter();
    assert_eq!(next_start, 4);

    let (tree2, cfgs2) = p1.root.build_tree_with_configs_starting_at(next_start);
    assert_eq!(cfgs2.len(), 3);
    for id in tree1.ordered_pane_ids() {
        assert!(!tree2.panes.contains_key(&id));
    }

    store.upsert(p1.clone());
    assert_eq!(store.presets.len(), 1);
    assert_eq!(store.startup_preset().map(|p| p.name.as_str()), Some("Dev Set"));

    let mut p2 = WorkspacePreset::default();
    p2.name = "Ops Set".to_string();
    store.upsert(p2);
    store.set_startup_preset(Some("Ops Set"));
    assert_eq!(store.startup_preset().map(|p| p.name.as_str()), Some("Ops Set"));
    assert!(!store.find_by_name("Dev Set").unwrap().run_on_startup);

    assert!(store.presets[0].root.remove_pane_at(1));
    assert_eq!(store.presets[0].root.pane_count(), 2);

    store.remove_by_name("Dev Set");
    assert_eq!(store.presets.len(), 1);
}
