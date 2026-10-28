use super::*;

#[test]
fn test_poll_mode_parsing_and_scheduler() {
    assert_eq!(PollMode::from_str_opt("event"), Some(PollMode::EventDriven));
    assert_eq!(
        PollMode::from_str_opt("event-driven"),
        Some(PollMode::EventDriven)
    );
    assert_eq!(
        PollMode::from_str_opt("poll"),
        Some(PollMode::ContinuousFps)
    );
    assert_eq!(
        PollMode::from_str_opt("continuous"),
        Some(PollMode::ContinuousFps)
    );

    let mut sched = RepaintScheduler::new(PollMode::EventDriven, 60);
    assert_eq!(sched.next_repaint_delay(false, false), None);
    assert_eq!(sched.next_repaint_delay(true, true), None);
    assert!(sched.next_repaint_delay(false, true).is_some());

    let mut cont = RepaintScheduler::new(PollMode::ContinuousFps, 30);
    assert_eq!(cont.next_repaint_delay(true, false), None);
    assert_eq!(
        cont.next_repaint_delay(false, false),
        Some(Duration::from_millis(33))
    );
}
