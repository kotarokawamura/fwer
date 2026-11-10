use super::*;

#[test]
fn test_compute_clamped_zoom() {
    // Base font 14px -> min 6.0/14.0 = 0.42857, max 3.5 (14*3.5 = 49px <= 64px)
    let z_max = compute_clamped_zoom(5.0, 14.0);
    assert!((z_max - 3.5).abs() < 1e-5);

    let z_min = compute_clamped_zoom(0.1, 14.0);
    assert!((z_min * 14.0 - 6.0).abs() < 1e-4);

    // Base font 24px -> max capped by 64px (64 / 24 = 2.6666)
    let z_capped = compute_clamped_zoom(3.5, 24.0);
    assert!((z_capped * 24.0 - 64.0).abs() < 1e-4);
}

#[test]
fn test_encode_terminal_key_bytes() {
    assert_eq!(
        encode_terminal_key_bytes("c", true, false, false),
        Some(vec![0x03])
    );
    assert_eq!(
        encode_terminal_key_bytes("space", true, false, false),
        Some(vec![0x00])
    );
    assert_eq!(
        encode_terminal_key_bytes("x", false, false, true),
        Some(vec![0x1B, b'x'])
    );
    assert_eq!(
        encode_terminal_key_bytes("tab", false, true, false),
        Some(b"\x1b[Z".to_vec())
    );
    assert_eq!(
        encode_terminal_key_bytes("up", false, false, false),
        Some(b"\x1b[A".to_vec())
    );
    assert_eq!(
        encode_terminal_key_bytes("f1", false, false, false),
        Some(b"\x1bOP".to_vec())
    );
    assert_eq!(
        encode_terminal_key_bytes("f12", false, false, false),
        Some(b"\x1b[24~".to_vec())
    );
}

#[test]
fn test_pane_selector_half_second_steps_and_keys() {
    use crate::ui::pane_selector::{
        SelectorKeyAction, half_steps_to_secs, map_selector_key, next_selector_index,
        timeout_secs_to_half_steps,
    };

    assert_eq!(timeout_secs_to_half_steps(0.0), 0);
    assert_eq!(timeout_secs_to_half_steps(0.5), 1);
    assert_eq!(timeout_secs_to_half_steps(1.5), 3);
    assert_eq!(half_steps_to_secs(3), 1.5);
    assert_eq!(next_selector_index(0, 2, 1), 1);
    assert_eq!(next_selector_index(1, 2, 1), 0);
    assert_eq!(next_selector_index(0, 2, -1), 1);
    assert_eq!(
        map_selector_key(gdk::Key::j),
        Some(SelectorKeyAction::MoveNext)
    );
    assert_eq!(
        map_selector_key(gdk::Key::Up),
        Some(SelectorKeyAction::MovePrev)
    );
    assert_eq!(
        map_selector_key(gdk::Key::Tab),
        Some(SelectorKeyAction::Confirm)
    );
    assert_eq!(
        map_selector_key(gdk::Key::Escape),
        Some(SelectorKeyAction::Cancel)
    );
}

#[test]
fn test_empty_pane_eof_ctrl_d_key() {
    use crate::ui::empty_pane::is_empty_pane_eof_close_key;

    assert!(is_empty_pane_eof_close_key(
        gdk::Key::d,
        gdk::ModifierType::CONTROL_MASK
    ));
    assert!(is_empty_pane_eof_close_key(
        gdk::Key::D,
        gdk::ModifierType::CONTROL_MASK
    ));
    assert!(!is_empty_pane_eof_close_key(
        gdk::Key::d,
        gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
    ));
    assert!(!is_empty_pane_eof_close_key(
        gdk::Key::d,
        gdk::ModifierType::empty()
    ));
}
