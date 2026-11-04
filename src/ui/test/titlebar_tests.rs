use super::*;

#[test]
fn test_grip_button_mode_transitions() {
    let id1 = TileId(1);
    let m1 = next_mode_on_grip_click(InteractionMode::Normal, id1, false);
    assert_eq!(m1, InteractionMode::MovingPane(id1));

    // Clicking again cancels MovingPane
    let m2 = next_mode_on_grip_click(m1, id1, false);
    assert_eq!(m2, InteractionMode::Normal);

    // Right click toggles SelectingBroadcast
    let b1 = next_mode_on_grip_click(InteractionMode::Normal, id1, true);
    assert_eq!(b1, InteractionMode::SelectingBroadcast(id1));

    let b2 = next_mode_on_grip_click(b1, id1, true);
    assert_eq!(b2, InteractionMode::Normal);
}

#[test]
fn test_titlebar_colors_active_inactive_and_broadcast() {
    let settings = ThemeSettings::default();
    let ctx_active = PaneContext {
        tile_id: TileId(1),
        is_active: true,
        is_top_right: true,
        broadcast_group: 0,
        mode: InteractionMode::Normal,
    };
    let (bg, fg) = resolve_titlebar_colors(&ctx_active, &settings);
    assert_eq!(bg, RgbColor(117, 33, 0));
    assert_eq!(fg, RgbColor(255, 255, 255));

    let ctx_bcast = PaneContext {
        tile_id: TileId(1),
        is_active: true,
        is_top_right: false,
        broadcast_group: 2,
        mode: InteractionMode::Normal,
    };
    let (bg_bcast, _) = resolve_titlebar_colors(&ctx_bcast, &settings);
    assert_eq!(bg_bcast, RgbColor(16, 142, 92));

    let ctx_inactive = PaneContext {
        tile_id: TileId(1),
        is_active: false,
        is_top_right: true,
        broadcast_group: 0,
        mode: InteractionMode::Normal,
    };
    let (bg_inact, fg_inact) = resolve_titlebar_colors(&ctx_inactive, &settings);
    assert_eq!(bg_inact, RgbColor(75, 75, 75));
    assert_eq!(fg_inact, RgbColor(210, 210, 210));
}

#[test]
fn test_color_shading_helpers() {
    let base = RgbColor(100, 50, 10);
    assert_eq!(lighten_rgb(base, 20), RgbColor(120, 70, 30));
    assert_eq!(darken_rgb(base, 20), RgbColor(80, 30, 0));
    assert_eq!(
        blend_rgb(RgbColor(0, 0, 0), RgbColor(200, 100, 50), 0.5),
        RgbColor(100, 50, 25)
    );
}

#[test]
fn test_readonly_indicator_color_contrast() {
    let test_bgs = [
        RgbColor(117, 33, 0),    // Default active
        RgbColor(75, 75, 75),    // Default inactive
        RgbColor(25, 118, 210),  // Broadcast 1
        RgbColor(16, 142, 92),   // Broadcast 2
        RgbColor(138, 58, 185),  // Broadcast 3
        RgbColor(217, 119, 6),   // Broadcast 4 (Amber)
        RgbColor(0, 151, 167),   // Broadcast 5
        RgbColor(194, 24, 91),   // Broadcast 6
        RgbColor(250, 240, 137), // Light yellow custom bg
        RgbColor(245, 245, 245), // White/light gray custom bg
    ];
    for bg in test_bgs {
        let ro_color = resolve_readonly_indicator_color(bg);
        assert!(
            contrast_ratio(bg, ro_color) >= 3.5,
            "Expected high contrast >= 3.5 for bg {:?}, got {} with {:?}",
            bg,
            contrast_ratio(bg, ro_color),
            ro_color
        );
    }
}
