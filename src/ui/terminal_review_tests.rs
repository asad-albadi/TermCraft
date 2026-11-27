use super::*;
// These fixtures launch owned Chromium renderer processes or an SSH agent and
// server. Limit their overlap on constrained test hosts, before any stage timer starts.
#[cfg(unix)]
pub(super) fn owned_process_fixture_slot() -> std::sync::MutexGuard<'static, ()> {
    static SLOT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
pub(super) fn owned_workspace() -> (Context, tempfile::TempDir, Workspace) {
    let ctx = Context::default();
    let root = tempfile::tempdir().unwrap();
    let w = Workspace::new_context(&ctx, None, root.path().to_owned());
    (ctx, root, w)
}
/// Wait for a real child acknowledgement and any terminal mode it negotiated.
/// Callers keep their original absolute deadline, including multi-tab fixtures.
#[cfg(unix)]
pub(super) fn wait_for_fixture(
    tab: &mut TerminalTab,
    deadline: Instant,
    expected: &str,
    ready: impl Fn(&TerminalTab) -> bool,
) {
    loop {
        tab.poll();
        if ready(tab) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "owned fixture {:?} did not reach {expected}: running={}, error={}, screen={:?}",
            tab.session.name,
            tab.running(),
            tab.error,
            tab.screen.plain()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
pub(super) fn raw_terminal(w: &mut Workspace, name: &str) -> u64 {
    // Match the provisioned endpoint interpreter, including setup-python on CI,
    // and keep user Python environment/site hooks out of the owned fixture.
    w.start_with_launch(Session{name:name.into(),protocol:"Local".into(),..Default::default()},Some(connection::Launch{rdp_profile:None,program:"python3".into(),args:vec!["-I".into(),"-c".into(),"import os,tty\ntty.setraw(0)\nos.write(1,b'\\x1b[?2004hREADY')\nwhile True:\n data=os.read(0,32768)\n if not data: break\n os.write(1,b'\\r\\nCAP:'+data.hex().encode()+b'\\r\\n')".into()],directory:None}));
    let index = w.active;
    let deadline = Instant::now() + Duration::from_secs(3);
    wait_for_fixture(
        &mut w.tabs[index],
        deadline,
        "READY and bracketed paste",
        |tab| tab.screen.plain().contains("READY") && tab.screen.bracketed_paste,
    );
    w.tabs[index].id
}
#[cfg(unix)]
pub(super) fn frame(ctx: &Context, w: &mut Workspace, events: Vec<Event>) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 900.))),
            events,
            ..Default::default()
        },
        |ui| w.terminals(ui),
    );
    output.textures_delta.clear();
    output
}
#[cfg(unix)]
pub(super) fn captured(tab: &TerminalTab) -> Vec<u8> {
    let plain = tab.screen.plain().replace('\n', "");
    let hex = plain
        .split("CAP:")
        .skip(1)
        .flat_map(|s| s.chars().take_while(|c| c.is_ascii_hexdigit()))
        .collect::<String>();
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[cfg(unix)]
#[test]
fn native_copy_event_interrupts_without_selection_but_shift_copy_does_not() {
    let (ctx, _root, mut w) = owned_workspace();
    let id = raw_terminal(&mut w, "owned");
    frame(&ctx, &mut w, vec![]);
    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
    frame(
        &ctx,
        &mut w,
        vec![
            Event::ModifiersChanged(Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            }),
            Event::Copy,
        ],
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while captured(&w.tabs[0]).is_empty() && Instant::now() < deadline {
        w.tabs[0].poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        captured(&w.tabs[0]),
        b"\x03",
        "native normalized Ctrl+C must deliver ETX"
    );
    frame(
        &ctx,
        &mut w,
        vec![
            Event::ModifiersChanged(Modifiers {
                ctrl: true,
                command: true,
                shift: true,
                ..Default::default()
            }),
            Event::Copy,
        ],
    );
    w.tabs[0].poll();
    assert_eq!(captured(&w.tabs[0]), b"\x03");
}
#[cfg(unix)]
#[test]
fn cancelling_recorded_paste_records_logical_text_without_framing() {
    let (_ctx, _root, mut w) = owned_workspace();
    let id = raw_terminal(&mut w, "recording");
    w.tabs[0].recording = true;
    w.store.preferences.paste.line_delay_ms = 1000;
    w.queue_paste(id, "one\ntwo".into());
    w.resolve_paste(true);
    let start = Instant::now();
    w.tabs[0].tick_paste(start);
    w.tabs[0].cancel_paste();
    w.tabs[0].tick_paste(start + Duration::from_millis(1));
    assert_eq!(
        w.tabs[0].recorded, "one\n",
        "recorded macros must not retain incomplete paste delimiters"
    );
}
#[cfg(unix)]
#[test]
fn output_bursts_share_one_work_budget_and_keep_close_cancel_responsive() {
    let (ctx, _root, mut w) = owned_workspace();
    for name in ["flood-one", "flood-two"] {
        w.start_with_launch(Session{name:name.into(),protocol:"Local".into(),..Default::default()},Some(connection::Launch{rdp_profile:None,program:"python3".into(),args:vec!["-I".into(),"-c".into(),"import os,tty\ntty.setraw(0)\nos.write(1,b'READY')\nos.read(0,1)\nos.write(1,b'ordinary owned log line\\r\\n'*50000)\nos.read(0,1)".into()],directory:None}));
        let index = w.active;
        let deadline = Instant::now() + Duration::from_secs(2);
        wait_for_fixture(&mut w.tabs[index], deadline, "READY", |tab| {
            tab.screen.plain().contains("READY")
        });
    }
    let initial: Vec<_> = w.tabs.iter().map(|t| t.codec.source_bytes()).collect();
    for t in &mut w.tabs {
        t.send(b"!");
    }
    std::thread::sleep(Duration::from_millis(50));
    let mut largest = 0;
    let start = Instant::now();
    for _ in 0..4 {
        let before: u64 = w.tabs.iter().map(|t| t.codec.source_bytes()).sum();
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 900.))),
                ..Default::default()
            },
            |ui| {
                eframe::App::logic(&mut w, ui.ctx(), &mut eframe::Frame::_new_kittest());
                w.render(ui);
            },
        );
        out.textures_delta.clear();
        let after: u64 = w.tabs.iter().map(|t| t.codec.source_bytes()).sum();
        largest = largest.max(after - before);
        assert!(
            after - before <= 8192,
            "all tabs and logic/render passes must share one 8 KiB parse budget; parsed {} bytes",
            after - before
        );
    }
    assert!(
        w.tabs
            .iter()
            .zip(initial)
            .all(|(t, initial)| t.codec.source_bytes() > initial),
        "backlogged tabs must make fair progress"
    );
    w.close_tab(w.active);
    assert!(w.pending_session_close.is_some());
    w.resolve_session_close(false, &ctx);
    assert!(w.tabs.iter().all(TerminalTab::running));
    eprintln!(
        "I8 owned output-flood check: four GUI ticks plus close/cancel {:?}, largest shared input budget {largest} bytes (debug; no timing-only assertion)",
        start.elapsed()
    );
}
#[cfg(unix)]
const MOUSE_RELEASE_SCANNER: &str = r#"tail=b''
release=b'\x1b[<0;1;1m'
def release_received(chunk):
 global tail
 data=tail+chunk
 found=release in data
 tail=data[-(len(release)-1):]
 return found
"#;

#[cfg(unix)]
#[test]
fn mouse_release_fixture_recognizes_escape_split_across_reads() {
    let result = std::process::Command::new("python3")
        .args(["-I", "-c", &format!("{MOUSE_RELEASE_SCANNER}\nassert not release_received(b'noise\\x1b[<0;')\nassert release_received(b'1;1m')\nassert not release_received(b'noise')")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[cfg(unix)]
#[test]
fn refused_mouse_release_is_retried_after_transport_admission_recovers() {
    use super::terminal_mouse::Viewport;
    let (ctx, root, mut w) = owned_workspace();
    let resume = root.path().join("resume-reader");
    let trace = root.path().join("reader-progress");
    let program = format!(
        "import os,sys,tty,time\ntty.setraw(0)\nos.write(1,b'\\x1b[?1000h\\x1b[?1006hREADY')\ndata=os.read(0,4096)\nos.write(1,b'PRESS_ACK')\nwhile not os.path.exists(sys.argv[1]): time.sleep(.001)\n{MOUSE_RELEASE_SCANNER}\nreceived=0\nnext_trace=0\nwhile True:\n data=os.read(0,32768)\n if not data: break\n received+=len(data)\n released=release_received(data)\n now=time.monotonic()\n if released or now>=next_trace:\n  with open(sys.argv[2],'w') as trace: trace.write(str(received)+' '+data[-64:].hex())\n  next_trace=now+.05\n if released: os.write(1,b'RELEASE_ACK')"
    );
    w.start_with_launch(
        Session {
            protocol: "Local".into(),
            ..Default::default()
        },
        Some(connection::Launch {
            rdp_profile: None,
            program: "python3".into(),
            args: vec![
                "-I".into(),
                "-c".into(),
                program.into(),
                resume.to_str().unwrap().into(),
                trace.to_str().unwrap().into(),
            ],
            directory: None,
        }),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    wait_for_fixture(
        &mut w.tabs[0],
        deadline,
        "READY and SGR mouse mode",
        |tab| {
            tab.screen.plain().contains("READY")
                && tab.screen.mouse_mode().enabled()
                && tab.screen.mouse_mode().sgr
        },
    );
    let route = |ctx: &Context, w: &mut Workspace, events: Vec<Event>| {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 600.))),
                ..Default::default()
            },
            |ui| {
                ui.allocate_space(vec2(800., 600.));
                let view = Viewport {
                    clip: Rect::from_min_max(pos2(200., 100.), pos2(500., 400.)),
                    rows: vec![(0, Rect::from_min_max(pos2(200., 120.), pos2(500., 140.)))],
                    cell_width: 10.,
                    cols: 30,
                    layer: ui.layer_id(),
                };
                w.tabs[0].route_mouse(ctx, &view, true, &events, Modifiers::default());
            },
        );
        output.textures_delta.clear();
    };
    let pos = pos2(205., 130.);
    route(
        &ctx,
        &mut w,
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::default(),
        }],
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while !w.tabs[0].screen.plain().contains("PRESS_ACK") {
        w.tabs[0].poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    // The background writer can dequeue its first item just after try_send reports Full.
    // Establish sustained kernel/queue backpressure before asserting refusal of the release.
    // Queue admission is bounded by message count. Small messages exercise the
    // same Full/retry path without putting a megabyte of unrelated bulk input
    // ahead of the mouse event whose delivery this fixture needs to observe.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut full_samples = 0;
    let mut accepted_filler = 0;
    while full_samples < 5 {
        match w.tabs[0].process.as_mut().unwrap().write(&[b'x'; 1024]) {
            Ok(()) => {
                accepted_filler += 1024;
                full_samples = 0;
            }
            Err(error) => {
                assert!(error.contains("queue full"));
                full_samples += 1;
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert!(
            Instant::now() < deadline,
            "fixture could not establish stable backpressure"
        );
    }
    route(
        &ctx,
        &mut w,
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }],
    );
    assert!(
        w.tabs[0].pending_mouse.is_some(),
        "refused release must remain pending"
    );
    fs::write(&resume, b"resume").unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while !w.tabs[0].screen.plain().contains("RELEASE_ACK") && Instant::now() < deadline {
        route(&ctx, &mut w, vec![]);
        w.tabs[0].poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        w.tabs[0].screen.plain().contains("RELEASE_ACK"),
        "release ACK missing: accepted_filler={}, reader_progress={:?}, pending={}, running={}, error={}, screen={:?}",
        accepted_filler,
        fs::read_to_string(&trace),
        w.tabs[0].pending_mouse.is_some(),
        w.tabs[0].running(),
        w.tabs[0].error,
        w.tabs[0].screen.plain()
    );
    assert!(w.tabs[0].pending_mouse.is_none());
}
#[cfg(unix)]
#[test]
fn paste_then_enter_in_one_frame_keeps_wire_order() {
    let (ctx, _root, mut w) = owned_workspace();
    let id = raw_terminal(&mut w, "ordered");
    frame(&ctx, &mut w, vec![]);
    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
    frame(
        &ctx,
        &mut w,
        vec![
            Event::Paste("hello".into()),
            Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            },
        ],
    );
    w.tabs[0].tick_paste(Instant::now());
    let deadline = Instant::now() + Duration::from_secs(2);
    while captured(&w.tabs[0]).len() < 18 && Instant::now() < deadline {
        w.tabs[0].poll();
        w.tabs[0].tick_paste(Instant::now());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(captured(&w.tabs[0]), b"\x1b[200~hello\x1b[201~\r");
}
#[cfg(unix)]
#[test]
fn cursor_report_waits_until_the_paste_transaction_ends() {
    let (_ctx, _root, mut w) = owned_workspace();
    let id = raw_terminal(&mut w, "report-order");
    w.store.preferences.paste.line_delay_ms = 1000;
    w.queue_paste(id, "one\ntwo".into());
    w.resolve_paste(true);
    let now = Instant::now();
    w.tabs[0].tick_paste(now);
    w.tabs[0].screen.feed(b"\x1b[6n");
    let reply = w.tabs[0].screen.replies.clone();
    w.tabs[0].poll();
    w.tabs[0].tick_paste(now + Duration::from_millis(1000));
    let expected = [b"\x1b[200~one\ntwo\x1b[201~".as_slice(), &reply].concat();
    let deadline = Instant::now() + Duration::from_secs(2);
    while captured(&w.tabs[0]).len() < expected.len() && Instant::now() < deadline {
        w.tabs[0].poll();
        w.tabs[0].tick_paste(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        captured(&w.tabs[0]),
        expected,
        "protocol replies must not become bracketed-paste payload"
    );
}
#[cfg(unix)]
#[test]
fn asynchronous_clipboard_keeps_original_targets_across_focus_and_broadcast_changes() {
    let (ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "clipboard-A");
    let b = raw_terminal(&mut w, "clipboard-B");
    w.active = 0;
    w.broadcast = false;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    w.request_clipboard_with(a, &ctx, move || {
        receive
            .recv()
            .map_err(|_| "owned clipboard provider closed".into())
    });
    w.active = 1;
    w.enable_broadcast_all();
    send.send("bound-to-A".into()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while captured(&w.tabs[0]).is_empty() && Instant::now() < deadline {
        w.poll_clipboard();
        for tab in &mut w.tabs {
            tab.tick_paste(Instant::now());
            tab.poll();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(captured(&w.tabs[0]), b"\x1b[200~bound-to-A\x1b[201~");
    assert_eq!(w.tabs[1].id, b);
    assert!(
        captured(&w.tabs[1]).is_empty(),
        "later focus or broadcast changes must not retarget clipboard text"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while w.clipboard_busy.load(std::sync::atomic::Ordering::Acquire) && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(!w.clipboard_busy.load(std::sync::atomic::Ordering::Acquire));
    w.broadcast = false;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    w.request_clipboard_with(a, &ctx, move || {
        receive
            .recv()
            .map_err(|_| "owned clipboard provider closed".into())
    });
    w.force_close_tab(0);
    send.send("must-be-discarded".into()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while w.clipboard_busy.load(std::sync::atomic::Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    w.poll_clipboard();
    w.tabs[0].tick_paste(Instant::now());
    w.tabs[0].poll();
    assert_eq!(w.tabs[0].id, b);
    assert!(captured(&w.tabs[0]).is_empty());
    assert!(w.pending_paste.is_none());
}
#[cfg(unix)]
#[test]
fn zero_budget_cannot_finalize_a_decoder_with_unread_transport_tail() {
    let (_ctx, _root, mut w) = owned_workspace();
    raw_terminal(&mut w, "decoder-tail");
    assert!(w.tabs[0].codec.feed(&[0xc3]).is_empty());
    // Model the authoritative exit notification arriving before its queued output tail is displayed.
    w.tabs[0].pending_output = Some((vec![0xa9], 0));
    w.tabs[0].output_ended = true;
    w.tabs[0].poll_with_budget(0);
    assert!(
        !w.tabs[0].screen.plain().contains('�'),
        "a skipped parse tick must not prematurely flush a pending character"
    );
    w.tabs[0].poll_with_budget(1);
    assert!(w.tabs[0].screen.plain().contains('é'));
}

#[cfg(unix)]
#[test]
fn telnet_paste_and_enter_same_frame_use_the_mode_at_delivery() {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
    };

    fn read_wire(stream: &mut TcpStream, size: usize) -> Vec<u8> {
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes).unwrap();
        bytes
    }

    for binary_after_frame in [true, false] {
        let (ctx, _root, mut w) = owned_workspace();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        w.start(Session {
            protocol: "Telnet".into(),
            host: "127.0.0.1".into(),
            port: listener.local_addr().unwrap().port(),
            ..Session::default()
        });
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "Telnet connection timed out");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(&[255, 251, 1]).unwrap();
        assert_eq!(read_wire(&mut stream, 3), [255, 253, 1]);
        let tab = &mut w.tabs[0];
        tab.recording = true;
        let id = tab.id;
        frame(&ctx, &mut w, vec![]);
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
        frame(
            &ctx,
            &mut w,
            vec![
                Event::Paste("show status".into()),
                Event::Text("+".into()),
                Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                Event::Text("next".into()),
            ],
        );
        assert!(w.tabs[0].paste.is_some());
        if binary_after_frame {
            stream.write_all(&[255, 253, 0]).unwrap();
            assert_eq!(read_wire(&mut stream, 3), [255, 251, 0]);
        }
        w.tabs[0].tick_paste(Instant::now());
        let expected: &[u8] = if binary_after_frame {
            b"show status+\rnext"
        } else {
            b"show status+\r\nnext"
        };
        assert_eq!(read_wire(&mut stream, expected.len()), expected);
        assert_eq!(w.tabs[0].recorded, "show status+\rnext");
        assert!(w.tabs[0].after_paste.is_none());
    }
}
