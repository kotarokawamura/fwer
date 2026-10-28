use super::*;

#[test]
fn test_ansi_palette_conversion() {
    assert_eq!(ansi_idx_to_rgb(0), RgbColor(40, 44, 52));
    assert_eq!(ansi_idx_to_rgb(15), RgbColor(255, 255, 255));
    // 256-color grayscale
    assert_eq!(ansi_idx_to_rgb(232), RgbColor(8, 8, 8));
}

#[test]
fn test_vt100_reflow_on_resize() {
    let mut vte = Vt100Adapter::new(5, 10);
    // Write a 16-char logical line that wraps on a 10-col screen
    vte.process_bytes(b"ABCDEFGHIJKLMNOP");
    assert!(vte.visible_row(0, 0).unwrap().wrapped);

    // Resize wider to 20 columns: the wrapped line should reflow into a single row!
    vte.resize(5, 20);
    let row0 = vte.visible_row(0, 0).unwrap();
    assert!(!row0.wrapped);
    let text0 = vte.extract_full_selected_text((0, 0), (0, 19));
    assert_eq!(text0, "ABCDEFGHIJKLMNOP");
}

#[test]
fn test_vt100_clear_scrollback_preserves_screen() {
    let mut vte = Vt100Adapter::new(3, 10);
    vte.process_bytes(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
    assert_eq!(vte.scrollback_len(), 2);

    vte.clear_scrollback();
    assert_eq!(vte.scrollback_len(), 0);
    let screen_text = vte.extract_full_selected_text((0, 0), (2, 9));
    assert_eq!(screen_text, "line3\nline4\nline5");
}

#[test]
fn test_alternate_screen_and_bracketed_paste() {
    let mut vte = Vt100Adapter::new(4, 10);
    assert!(!vte.alternate_screen());
    assert!(!vte.bracketed_paste_enabled());

    vte.process_bytes(b"\x1b[?1049h\x1b[?2004h");
    assert!(vte.alternate_screen());
    assert!(vte.bracketed_paste_enabled());

    vte.process_bytes(b"\x1b[?1049l\x1b[?2004l");
    assert!(!vte.alternate_screen());
    assert!(!vte.bracketed_paste_enabled());
}
