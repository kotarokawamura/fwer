use super::*;
use std::time::{Duration, Instant};

#[test]
fn test_local_pty_command_execution_and_cwd() {
    let rows = NonZeroU16::new(24).unwrap();
    let cols = NonZeroU16::new(80).unwrap();
    let mut session =
        LocalPtySession::spawn(rows, cols, None, Some("printf 'HELLO_FWER'"), None)
            .expect("spawn LocalPtySession");

    assert!(session.current_working_dir().is_some());

    let start = Instant::now();
    let mut collected = Vec::new();
    while start.elapsed() < Duration::from_secs(2) {
        let out = session.read_output();
        if !out.is_empty() {
            collected.extend_from_slice(&out);
            if String::from_utf8_lossy(&collected).contains("HELLO_FWER") {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(String::from_utf8_lossy(&collected).contains("HELLO_FWER"));
}
