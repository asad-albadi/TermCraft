use harbor_workspace::games::screensaver::ScreensaverState;
use std::time::Duration;

#[test]
fn screensaver_requires_explicit_opt_in_and_exact_idle_threshold() {
    let mut saver = ScreensaverState::default();
    saver.tick(Duration::from_secs(86400));
    assert!(!saver.active());
    saver.configure(Some(Duration::from_secs(60)));
    saver.tick(Duration::from_secs(59));
    assert!(!saver.active());
    saver.tick(Duration::from_secs(1));
    assert!(saver.active());
    assert!(saver.on_input());
    assert!(!saver.active());
    saver.tick(Duration::from_secs(59));
    assert!(!saver.active());
}

#[test]
fn screensaver_disable_and_reconfigure_reset_idle() {
    let mut saver = ScreensaverState::default();
    saver.configure(Some(Duration::ZERO));
    saver.tick(Duration::MAX);
    assert!(!saver.active(), "zero timeout must never start a saver");
    saver.configure(Some(Duration::from_secs(1)));
    saver.tick(Duration::MAX);
    assert!(saver.active());
    saver.configure(None);
    assert!(!saver.active());
    assert!(!saver.on_input());
}
