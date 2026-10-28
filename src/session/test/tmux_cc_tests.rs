use super::*;

#[test]
fn test_tmux_control_session_skeleton() {
    let rows = NonZeroU16::new(24).unwrap();
    let cols = NonZeroU16::new(80).unwrap();
    let mut tmux = TmuxControlSession::new("%1", rows, cols);
    assert!(tmux.is_alive());

    tmux.push_simulated_output(b"tmux output");
    assert_eq!(tmux.read_output(), b"tmux output");

    tmux.write_input(b"ls\n");
    assert_eq!(tmux.sent_commands.len(), 1);
    assert!(tmux.sent_commands[0].starts_with("send-keys -t %1 -H"));

    tmux.resize(NonZeroU16::new(30).unwrap(), NonZeroU16::new(100).unwrap());
    assert_eq!(tmux.rows.get(), 30);
    assert_eq!(tmux.cols.get(), 100);
}
