use super::*;

#[test]
fn test_parse_color_hex_and_rgb() {
    assert_eq!(parse_color("#752100"), Some(RgbColor(117, 33, 0)));
    assert_eq!(parse_color("#FFFFFF"), Some(RgbColor(255, 255, 255)));
    assert_eq!(parse_color("rgb(25, 118, 210)"), Some(RgbColor(25, 118, 210)));
    assert_eq!(parse_color(""), None);
    assert_eq!(parse_color("invalid"), None);
}

#[test]
fn test_split_shortcut_bindings_with_comma_key() {
    let bindings = split_shortcut_bindings("Ctrl+, / Super+,");
    assert_eq!(bindings, vec!["Ctrl+,", "Super+,"]);

    let multi = split_shortcut_bindings("Ctrl+Shift+C, Super+C | Alt+C");
    assert_eq!(multi, vec!["Ctrl+Shift+C", "Super+C", "Alt+C"]);
}

#[test]
fn test_matches_shortcut_aliases_and_plus_equals() {
    assert!(matches_shortcut("Ctrl+,", ",", true, false, false, false));
    assert!(matches_shortcut("Control+Shft+C", "c", true, true, false, false));
    assert!(matches_shortcut("Win+T", "t", false, false, false, true));
    assert!(matches_shortcut("Ctrl+=", "+", true, true, false, false));
    assert!(matches_shortcut("Ctrl+=", "=", true, false, false, false));
}

#[test]
fn test_broadcast_group_palette_cycles() {
    assert_eq!(broadcast_group_title_bg(1, true), RgbColor(25, 118, 210));
    assert_eq!(broadcast_group_title_bg(1, false), RgbColor(21, 75, 135));
    assert_eq!(broadcast_group_title_bg(7, true), RgbColor(25, 118, 210));
}

#[test]
fn test_is_valid_font_bytes() {
    assert!(!is_valid_font_bytes(b"short"));
    // Valid OTTO header with 1 table record (12 + 16 = 28 bytes)
    let mut otto = vec![b'O', b'T', b'T', b'O', 0x00, 0x01, 0, 0, 0, 0, 0, 0];
    otto.extend_from_slice(&[0u8; 16]);
    assert!(is_valid_font_bytes(&otto));
}
