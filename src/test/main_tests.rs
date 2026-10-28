use super::*;

#[test]
fn test_cli_flags_and_poll_mode_resolution() {
    let opts = parse_cli_args(["-M", "-dd", "-f", "120", "--prefs"]);
    assert!(opts.maximize);
    assert_eq!(opts.debug_level, 2);
    assert_eq!(opts.target_fps, Some(120));
    assert!(opts.open_prefs);
    assert_eq!(opts.resolve_poll_mode(), PollMode::ContinuousFps);

    let opts_explicit = parse_cli_args(["-f", "60", "--event"]);
    assert_eq!(opts_explicit.resolve_poll_mode(), PollMode::EventDriven);
}

#[test]
fn test_cli_command_quoting() {
    let opts = parse_cli_args(["-e", "bash", "-c", "echo hello world"]);
    assert_eq!(
        opts.startup_command,
        Some("bash -c 'echo hello world'".to_string())
    );

    let opts_eq = parse_cli_args(["--command=htop -d 10"]);
    assert_eq!(opts_eq.startup_command, Some("htop -d 10".to_string()));
}

#[test]
fn test_version_and_help_output() {
    assert_eq!(format_version_string(), "termin 0.0.3");
    assert!(format_help_string().contains("--maximize"));
    assert!(format_help_string().contains("--prefs"));
}
