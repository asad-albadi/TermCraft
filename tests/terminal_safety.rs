use harbor_workspace::terminal::paste::{
    CloseController, CloseDecision, PasteController, PasteDecision, PasteSettings,
};
use std::time::{Duration, Instant};
fn controller(decision: PasteDecision) -> PasteController {
    match decision {
        PasteDecision::Send(c) | PasteDecision::Confirm(c) => c,
        PasteDecision::Reject(e) => panic!("unexpected rejection {e}"),
    }
}
#[test]
fn cancelled_multiline_paste_sends_zero_bytes() {
    let now = Instant::now();
    let mut c = controller(PasteController::begin(
        "one\ntwo".into(),
        PasteSettings::default(),
    ));
    assert!(c.tick(now).is_empty());
    assert!(c.cancel().is_empty());
    assert!(c.tick(now + Duration::from_secs(10)).is_empty());
}
#[test]
fn delayed_bracketed_paste_uses_one_delimiter_pair() {
    let now = Instant::now();
    let mut c = controller(PasteController::begin(
        "one\ntwo\nthree".into(),
        PasteSettings {
            bracketed: true,
            line_delay_ms: 100,
            ..Default::default()
        },
    ));
    c.confirm(now);
    let mut writes = c.tick(now);
    assert_eq!(writes, b"\x1b[200~one\n");
    assert!(c.tick(now + Duration::from_millis(99)).is_empty());
    writes.extend(c.tick(now + Duration::from_millis(100)));
    assert!(!c.finished());
    writes.extend(c.tick(now + Duration::from_millis(200)));
    assert!(c.finished());
    assert_eq!(writes, b"\x1b[200~one\ntwo\nthree\x1b[201~");
    assert_eq!(writes.windows(6).filter(|b| *b == b"\x1b[200~").count(), 1);
    assert_eq!(writes.windows(6).filter(|b| *b == b"\x1b[201~").count(), 1);
}
#[test]
fn cancel_close_retains_running_session() {
    let mut close = CloseController::default();
    assert_eq!(close.begin(true, true), CloseDecision::Confirm);
    assert_eq!(close.resolve(false), CloseDecision::Cancel);
    assert_eq!(close.begin(false, true), CloseDecision::Close);
    assert_eq!(close.begin(true, false), CloseDecision::Close);
}
#[test]
fn paste_chunks_and_interrupts_are_bounded_and_injection_is_rejected() {
    let now = Instant::now();
    let text = "é".repeat(20000);
    let mut c = controller(PasteController::begin(
        text.clone(),
        PasteSettings {
            confirm_multiline: false,
            bracketed: true,
            ..Default::default()
        },
    ));
    let mut writes = Vec::new();
    while !c.finished() {
        let chunk = c.tick(now);
        assert!(chunk.len() <= 16384 + 12);
        writes.extend(chunk);
    }
    assert_eq!(
        writes,
        [b"\x1b[200~".as_slice(), text.as_bytes(), b"\x1b[201~"].concat()
    );
    let mut c = controller(PasteController::begin(
        "one\ntwo".into(),
        PasteSettings {
            confirm_multiline: false,
            bracketed: true,
            line_delay_ms: 100,
            ..Default::default()
        },
    ));
    assert_eq!(c.tick(now), b"\x1b[200~one\n");
    assert_eq!(c.cancel(), b"\x1b[201~");
    assert!(c.cancel().is_empty());
    assert!(c.tick(now + Duration::from_secs(1)).is_empty());
    assert!(matches!(
        PasteController::begin("a\x1b[201~b".into(), PasteSettings::default()),
        PasteDecision::Reject(_)
    ));
    assert!(matches!(
        PasteController::begin("x".repeat(4 * 1024 * 1024 + 1), PasteSettings::default()),
        PasteDecision::Reject(_)
    ));
}
#[test]
fn refused_transport_chunk_is_retried_without_skipping_or_orphan_delimiters() {
    let now = Instant::now();
    let settings = PasteSettings {
        bracketed: true,
        confirm_multiline: false,
        ..Default::default()
    };
    let mut c = controller(PasteController::begin("payload".into(), settings));
    let first = c.tick(now);
    c.retry_last_chunk();
    assert!(!c.finished());
    assert_eq!(c.tick(now), first);
    let mut c = controller(PasteController::begin("payload".into(), settings));
    let _ = c.tick(now);
    c.retry_last_chunk();
    assert!(c.cancel().is_empty());
}
#[test]
fn existing_transport_backpressure_messages_retry_but_closed_connections_stop() {
    use harbor_workspace::terminal::paste::retryable_input_error;
    assert!(retryable_input_error(
        "Terminal input queue full; retry when the application reads input."
    ));
    assert!(retryable_input_error(
        "SSH input queue is full. Wait for the server to receive pending input."
    ));
    assert!(retryable_input_error(
        "SSH connection is busy or disconnected. Wait, then retry."
    ));
    assert!(!retryable_input_error("SSH connection is disconnected."));
}
