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
