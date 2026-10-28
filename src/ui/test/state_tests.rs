use super::*;

#[test]
fn test_interaction_mode_and_pane_context() {
    let id1 = TileId(1);
    let id2 = TileId(2);

    let ctx_normal = PaneContext {
        tile_id: id1,
        is_active: true,
        is_top_right: true,
        broadcast_group: 0,
        mode: InteractionMode::Normal,
    };
    assert!(!ctx_normal.is_terminal_input_blocked());
    assert!(!ctx_normal.is_other_pane_blocked());

    let ctx_moving = PaneContext {
        tile_id: id1,
        is_active: true,
        is_top_right: true,
        broadcast_group: 0,
        mode: InteractionMode::MovingPane(id1),
    };
    assert!(ctx_moving.is_move_source());
    assert!(ctx_moving.is_terminal_input_blocked());

    let ctx_editing_other = PaneContext {
        tile_id: id2,
        is_active: false,
        is_top_right: false,
        broadcast_group: 0,
        mode: InteractionMode::EditingTitle(id1),
    };
    assert!(ctx_editing_other.is_other_pane_blocked());
    assert!(!ctx_editing_other.is_editing_this_title());
}

#[test]
fn test_workspace_tree_split_swap_top_right_and_broadcast() {
    use crate::ui::tree_ops::WorkspaceTree;

    let (mut tree, p1) = WorkspaceTree::with_initial_pane("Pane 1", None);
    assert_eq!(tree.top_right_pane_id(), Some(p1));

    // Horizontal split: p2 is on the right -> p2 becomes top-right
    let p2 = tree.split_pane(p1, true, "Pane 2", None).unwrap();
    assert_eq!(tree.top_right_pane_id(), Some(p2));

    // Vertical split on p2: p3 is below p2 -> p2 remains top-right
    let p3 = tree.split_pane(p2, false, "Pane 3", None).unwrap();
    assert_eq!(tree.top_right_pane_id(), Some(p2));

    // Swap p1 and p2 -> p1 is now top-right
    assert!(tree.swap_panes(p1, p2));
    assert_eq!(tree.top_right_pane_id(), Some(p1));

    // Broadcast group linking & auto-cleanup when singleton
    let grp = tree.link_broadcast_panes(p1, p3);
    assert_eq!(grp, 1);
    assert_eq!(tree.broadcast_peers(p1), vec![p3]);

    // Leaving broadcast group from p3 leaves p1 alone -> auto-cleans group to 0
    tree.leave_broadcast_group(p3);
    assert_eq!(tree.panes.get(&p1).unwrap().broadcast_group, 0);
}
