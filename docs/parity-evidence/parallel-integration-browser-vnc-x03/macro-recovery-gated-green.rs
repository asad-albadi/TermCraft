use super::*;
use harbor_workspace::macros::{
    Action as Input, Context as Tick, Event as Step, Player, Recorder, Script, Target,
};
use std::{collections::VecDeque, io::Read, path::Path};

const MAX_IMPORT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LIBRARY_SIZE: usize = 256;
const TERMINAL_ENTER: u32 = 13;

#[derive(Default)]
pub(super) struct TabState {
    capture: Capture,
    pending: Option<(Player, Instant)>,
    playback: Option<Playback>,
    output: String,
    terminal_focus: Option<(ViewportId, bool)>,
}

struct Playback {
    player: Player,
    actions: VecDeque<Input>,
    paste: bool,
    paste_rejected: bool,
    clock: Duration,
    last: Instant,
}

#[derive(Default)]
pub(super) struct Capture {
    recorder: Option<Recorder>,
    target: Option<Target>,
    start: Option<Instant>,
    error: Option<String>,
}
impl Capture {
    pub(super) fn active(&self) -> bool {
        self.recorder.is_some() && self.error.is_none()
    }

    fn start(&mut self, target: Target) {
        *self = Self {
            recorder: Some(Recorder::new(target, Duration::ZERO)),
            target: Some(target),
            start: Some(Instant::now()),
            error: None,
        };
    }

    pub(super) fn stop(&mut self, reason: &str) {
        if self.active() {
            self.error = Some(reason.into());
        }
    }

    pub(super) fn record(&mut self, input: Input) {
        if self.active() {
            let context = Tick {
                target: self.target.unwrap(),
                now: self.start.unwrap().elapsed(),
                authenticated: true,
                focused: true,
                output: "",
            };
            if let Err(error) = self.recorder.as_mut().unwrap().record(context, input) {
                self.error = Some(error);
            }
        }
    }
}
impl TabState {
    pub(super) fn terminal_focus(&mut self, ctx: &Context, focused: bool) {
        self.terminal_focus = Some((ctx.viewport_id(), focused));
    }

    pub(super) fn input_submitted(&mut self) {
        self.output.clear();
        if let Some(playback) = &mut self.playback {
            playback.player.input_submitted(playback.clock);
        }
    }

    pub(super) fn records_terminal(&self) -> bool {
        self.capture.active()
    }
    pub(super) fn owns_paste(&self) -> bool {
        self.playback
            .as_ref()
            .is_some_and(|playback| playback.paste)
    }
    pub(super) fn reject_paste(&mut self) {
        if let Some(playback) = self.playback.as_mut().filter(|playback| playback.paste) {
            playback.paste_rejected = true;
        }
    }
}

pub(super) fn supported_session(session: &Session) -> bool {
    match session.protocol.as_str() {
        "Local" => true,
        "SSH" => session.ssh_engine == SshEngine::Embedded,
        "RDP" => {
            !session.view_only
                && session.desktop_engine == harbor_workspace::model::DesktopEngine::Embedded
        }
        "VNC" => !session.view_only && session.vnc_options.embedded,
        _ => false,
    }
}

fn script_for(value: &Macro) -> Script {
    if let Some(script) = &value.script {
        return script.clone();
    }
    let mut text = value.text.clone();
    if value.append_enter {
        text.push('\n');
    }
    Script {
        version: 1,
        events: vec![Step::Input(Input::Text(text))],
    }
}

fn library(values: &[Macro]) -> Vec<(String, Script)> {
    values
        .iter()
        .map(|value| (value.name.clone(), script_for(value)))
        .collect()
}

pub(super) fn validate_library(values: &[Macro]) -> Result<(), String> {
    if values.len() > MAX_LIBRARY_SIZE {
        return Err("The macro library limit is 256 entries.".into());
    }
    let scripts = library(values);
    for (_, script) in &scripts {
        Player::new(
            Target {
                tab: 0,
                generation: 0,
            },
            script.clone(),
            &scripts,
            Duration::ZERO,
        )?;
    }
    Ok(())
}

pub(super) fn read_import(path: &Path) -> Result<Vec<Macro>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_IMPORT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("The macro file exceeds 4 MiB.".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

impl TerminalTab {
    fn macro_target(&self) -> Target {
        // Reconnect allocates a new, never-reused runtime ID.
        Target {
            tab: self.id,
            generation: self.id,
        }
    }

    fn macro_ready(&self) -> bool {
        supported_session(&self.session)
            && self.auth.is_none()
            && if let Some(desktop) = &self.desktop {
                desktop.macro_ready()
            } else {
                self.ready_for_input() && (self.session.protocol == "Local" || self.ssh.is_some())
            }
    }

    pub(super) fn record_macro_text(&mut self, text: &str) {
        if text.is_empty() || !self.macro_state.capture.active() {
            return;
        }
        if self.macro_ready() {
            let input = if text == "\r" {
                Input::Key {
                    code: TERMINAL_ENTER,
                    pressed: true,
                }
            } else {
                Input::Text(text.into())
            };
            self.macro_state.capture.record(input);
        } else {
            self.macro_state
                .capture
                .stop("The terminal is no longer ready for input.");
        }
    }

    pub(super) fn capture_macro_output(&mut self, output: &str) {
        let watching = self.macro_state.playback.is_some() || self.macro_state.pending.is_some();
        if watching && self.macro_ready() && self.macro_state.output.len() <= 1024 * 1024 {
            self.macro_state.output.push_str(output);
        }
    }

    fn capture(&mut self) -> &mut Capture {
        if let Some(desktop) = &mut self.desktop {
            &mut desktop.macro_capture
        } else {
            &mut self.macro_state.capture
        }
    }
}

impl Workspace {
    pub(super) fn queue_startup_macro(&mut self, id: u64) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        let Some(name) = tab.session.startup_macro.clone() else {
            return;
        };
        if let Err(error) = self.queue_named_macro(id, &name) {
            self.message(format!("Startup macro: {error}"));
        }
    }

    pub(super) fn queue_named_macro(&mut self, id: u64, name: &str) -> Result<(), String> {
        let mut matches = self.store.macros.iter().filter(|value| value.name == name);
        let value = matches.next().ok_or("Saved macro was not found.")?;
        if matches.next().is_some() {
            return Err("Macro name is ambiguous; rename duplicates first.".into());
        }
        self.queue_macro(id, &value.clone())
    }

    pub(super) fn queue_macro(&mut self, id: u64, value: &Macro) -> Result<(), String> {
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == id)
            .ok_or("Session closed.")?;
        if !supported_session(&tab.session) {
            return Err(
                "Choose a local terminal, embedded SSH, or a writable embedded desktop.".into(),
            );
        }
        if tab.macro_state.pending.is_some()
            || tab.macro_state.playback.is_some()
            || tab.capture().active()
        {
            return Err("Finish the session's current macro first.".into());
        }
        let script = script_for(value);
        let scripts = if script
            .events
            .iter()
            .any(|event| matches!(event, Step::Call { .. }))
        {
            library(&self.store.macros)
        } else {
            Vec::new()
        };
        let player = Player::new(tab.macro_target(), script, &scripts, Duration::ZERO)?;
        let desktop = matches!(tab.session.protocol.as_str(), "RDP" | "VNC");
        if !player.events_match(|event| match event {
            Step::WaitPattern { .. } => !desktop,
            Step::Input(Input::Pointer { .. }) => desktop,
            Step::Input(Input::Key { code, .. }) => desktop || *code == TERMINAL_ENTER,
            Step::Input(Input::Text(text)) if desktop => {
                text.len() <= 4096 && !text.chars().any(char::is_control)
            }
            _ => true,
        }) {
            return Err("This macro contains events that the selected session cannot run.".into());
        }
        tab.macro_state.pending = Some((player, Instant::now()));
        Ok(())
    }

    pub(super) fn cancel_macro(&mut self, id: u64) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        if self.tabs[index].macro_state.owns_paste() {
            if self.tabs[index].paste_waiting {
                self.resolve_paste(false);
            }
            self.tabs[index].cancel_paste();
        }
        let tab = &mut self.tabs[index];
        tab.macro_state.pending = None;
        tab.macro_state.output.clear();
        if let Some(mut playback) = tab.macro_state.playback.take() {
            if let Some(desktop) = &mut tab.desktop {
                desktop.macro_playing = false;
            }
            for action in playback.player.cancel() {
                if let Some(desktop) = &mut tab.desktop {
                    desktop.cancel_macro_input(action);
                }
            }
        }
        tab.capture()
            .stop("The session ended or macro recording was canceled.");
    }

    pub(super) fn poll_macros(&mut self, ctx: &Context) {
        for index in 0..self.tabs.len() {
            let id = self.tabs[index].id;
            if let Err(error) = self.poll_macro(index, ctx) {
                self.cancel_macro(id);
                self.message(error);
            }
        }
    }

    fn poll_macro(&mut self, index: usize, ctx: &Context) -> Result<(), String> {
        let tab = &mut self.tabs[index];
        let id = tab.id;
        let ready = tab.macro_ready();
        if !ready {
            tab.capture()
                .stop("The session disconnected or requires authentication.");
            if tab.macro_state.playback.is_some() {
                return Err(
                    "Macro canceled because the session disconnected or requires authentication."
                        .into(),
                );
            }
            if let Some((_, queued)) = &tab.macro_state.pending {
                ctx.request_repaint_after(Duration::from_millis(30));
                if !tab.error.is_empty()
                    || tab.output_ended
                    || tab
                        .desktop
                        .as_ref()
                        .is_some_and(|desktop| desktop.handle.is_finished())
                    || queued.elapsed() > Duration::from_secs(600)
                {
                    return Err("Macro canceled because the session did not become ready.".into());
                }
            }
            return Ok(());
        }
        if tab.input_busy() && !tab.macro_state.owns_paste() {
            if tab.macro_state.pending.is_some() || tab.macro_state.playback.is_some() {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            return Ok(());
        }
        if let Some((player, _)) = tab.macro_state.pending.take() {
            if let Some(desktop) = &mut tab.desktop {
                desktop.release();
                desktop.macro_playing = true;
            }
            tab.macro_state.playback = Some(Playback {
                player,
                actions: VecDeque::new(),
                paste: false,
                paste_rejected: false,
                clock: Duration::ZERO,
                last: Instant::now(),
            });
        }
        let target = tab.macro_target();
        let Some(playback) = &mut tab.macro_state.playback else {
            return Ok(());
        };
        ctx.request_repaint_after(Duration::from_millis(16));
        if playback.paste_rejected {
            return Err("Macro canceled because its paste was declined.".into());
        }
        let elapsed = playback.last.elapsed();
        playback.last = Instant::now();
        if playback.paste && (tab.paste_waiting || tab.paste.is_some() || tab.paste_end.is_some()) {
            return Ok(());
        }
        if !playback.paste {
            playback.clock = playback.clock.saturating_add(elapsed);
        }
        playback.paste = false;
        if let Some(desktop) = &mut tab.desktop {
            if !desktop.drain_macro_input()? {
                return Ok(());
            }
        }
        if playback.actions.is_empty() {
            let focused = self
                .painted_terminals
                .get(&id)
                .is_some_and(|&(window, viewport)| {
                    if self.windows.window(id) != window
                        || (window == 0 && self.page != Page::Terminal)
                    {
                        return false;
                    }
                    if ctx.input(|input| {
                        input.raw.viewports.get(&viewport).is_some_and(|info| {
                            info.focused == Some(false) || info.visible() == Some(false)
                        })
                    }) {
                        return false;
                    }
                    tab.desktop.as_ref().map_or_else(
                        || tab.macro_state.terminal_focus == Some((viewport, true)),
                        |desktop| desktop.macro_focused(),
                    )
                });
            let result = playback.player.tick(Tick {
                target,
                now: playback.clock,
                authenticated: ready,
                focused,
                output: &tab.macro_state.output,
            });
            tab.macro_state.output.clear();
            if result.is_err() {
                for action in playback.player.take_cleanup() {
                    if let Some(desktop) = &mut tab.desktop {
                        desktop.cancel_macro_input(action);
                    }
                }
            }
            playback.actions.extend(result?);
        }
        for _ in 0..8 {
            let Some(action) = tab
                .macro_state
                .playback
                .as_mut()
                .unwrap()
                .actions
                .pop_front()
            else {
                break;
            };
            if let Some(desktop) = &mut tab.desktop {
                if !desktop.play_macro_action(action)? {
                    return Ok(());
                }
            } else if matches!(
                action,
                Input::Key {
                    code: TERMINAL_ENTER,
                    pressed: true
                }
            ) {
                tab.send_user_bytes(b"\r", false)?;
                tab.macro_state.input_submitted();
            } else if let Input::Text(text) = action {
                if text.contains(['\n', '\r']) || text.len() > 16 * 1024 {
                    if self.pending_paste.is_some() {
                        tab.macro_state
                            .playback
                            .as_mut()
                            .unwrap()
                            .actions
                            .push_front(Input::Text(text));
                        return Ok(());
                    }
                    tab.macro_state.playback.as_mut().unwrap().paste = true;
                    self.queue_paste_to(
                        vec![id],
                        text,
                        session_confirmations::AfterPaste::default(),
                    );
                    if !self.tabs[index].paste_waiting && self.tabs[index].paste.is_none() {
                        return Err("Macro canceled because its paste could not start.".into());
                    }
                    return Ok(());
                } else {
                    tab.send_user_bytes(text.as_bytes(), false)?;
                    tab.macro_state.input_submitted();
                }
            }
        }
        let playback = tab.macro_state.playback.as_ref().unwrap();
        if playback.player.finished() && playback.actions.is_empty() {
            if let Some(desktop) = &mut tab.desktop {
                desktop.macro_playing = false;
            }
            tab.macro_state.playback = None;
            tab.macro_state.output.clear();
        }
        Ok(())
    }

    pub(super) fn macro_recording(&self) -> bool {
        self.tabs.iter().any(|tab| {
            tab.macro_state.capture.active()
                || tab
                    .desktop
                    .as_ref()
                    .is_some_and(|desktop| desktop.macro_capture.active())
        })
    }

    pub(super) fn macro_controls(&mut self, ui: &mut Ui) {
        if let Some(index) = self
            .tabs
            .iter_mut()
            .position(|tab| tab.capture().recorder.is_some())
        {
            let title = self.tabs[index].session.title();
            if ui.button(format!("Finish recording · {title}")).clicked() {
                self.finish_macro_recording(index);
                ui.close();
            }
        } else if ui
            .add_enabled(
                self.tabs
                    .get(self.active)
                    .is_some_and(TerminalTab::macro_ready),
                Button::new("Record active session"),
            )
            .clicked()
        {
            match self.start_macro_recording(self.active) {
                Ok(()) => self.page = Page::Terminal,
                Err(error) => self.message(error),
            }
            ui.close();
        }
        for index in 0..self.tabs.len() {
            let tab = &self.tabs[index];
            if (tab.macro_state.playback.is_some() || tab.macro_state.pending.is_some())
                && ui
                    .button(format!("Cancel playback · {}", tab.session.title()))
                    .clicked()
            {
                self.cancel_macro(tab.id);
                ui.close();
            }
        }
    }

    pub(super) fn start_macro_recording(&mut self, index: usize) -> Result<(), String> {
        let tab = self.tabs.get_mut(index).ok_or("Open a session first.")?;
        if !tab.macro_ready()
            || tab.macro_state.playback.is_some()
            || tab.macro_state.pending.is_some()
        {
            return Err("Choose a connected session with no macro in progress.".into());
        }
        let target = tab.macro_target();
        tab.capture().start(target);
        Ok(())
    }

    pub(super) fn finish_macro_recording(&mut self, index: usize) {
        let capture = std::mem::take(self.tabs[index].capture());
        if let Some(error) = capture.error {
            self.message(error);
        }
        if let Some(recorder) = capture.recorder {
            self.macro_form = Macro {
                name: "Recorded input".into(),
                script: Some(recorder.finish()),
                ..Macro::default()
            };
            self.macro_editing = None;
            self.macro_open = true;
        }
    }
}

pub(super) fn edit_events(ui: &mut Ui, script: &mut Script) {
    let mut remove = None;
    let mut swap = None;
    let count = script.events.len();
    ScrollArea::vertical().max_height(380.).show(ui, |ui| {
        for (index, event) in script.events.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{}.", index + 1));
                    match event {
                        Step::Input(Input::Text(text)) => {
                            ui.label("Send text");
                            ui.add(
                                TextEdit::multiline(text)
                                    .desired_rows(1)
                                    .desired_width(200.),
                            );
                        }
                        Step::Input(Input::Key { code, pressed }) => {
                            ui.label("Scan code (13 for terminal Enter)");
                            ui.add(DragValue::new(code).range(1..=511));
                            ui.checkbox(pressed, "Down");
                        }
                        Step::Input(Input::Pointer { x, y, buttons }) => {
                            ui.label("Pointer X / Y");
                            ui.add(DragValue::new(x));
                            ui.add(DragValue::new(y));
                            for (mask, label) in [(1, "Left"), (2, "Right"), (4, "Middle")] {
                                let mut held = *buttons & mask != 0;
                                if ui.checkbox(&mut held, label).changed() {
                                    *buttons ^= mask;
                                }
                            }
                        }
                        Step::Input(Input::ReleaseAll) => {
                            ui.label("Release held input");
                        }
                        Step::Wait { millis } => {
                            ui.label("Delay");
                            ui.add(DragValue::new(millis).range(0..=3_600_000).suffix(" ms"));
                        }
                        Step::WaitPattern {
                            pattern,
                            timeout_millis,
                        } => {
                            ui.label("Wait for terminal output");
                            ui.add(TextEdit::singleline(pattern).desired_width(140.));
                            timeout_control(ui, timeout_millis);
                        }
                        Step::WaitFocus { timeout_millis } => {
                            ui.label("Wait for session focus");
                            timeout_control(ui, timeout_millis);
                        }
                        Step::Call { name } => {
                            ui.label("Call macro");
                            ui.add(TextEdit::singleline(name).desired_width(140.));
                        }
                    }
                    if ui
                        .add_enabled(index > 0, Button::new("↑"))
                        .on_hover_text("Move event up")
                        .clicked()
                    {
                        swap = Some((index - 1, index));
                    }
                    if ui
                        .add_enabled(index + 1 < count, Button::new("↓"))
                        .on_hover_text("Move event down")
                        .clicked()
                    {
                        swap = Some((index, index + 1));
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
                ui.separator();
            });
        }
    });
    if let Some(index) = remove {
        script.events.remove(index);
    } else if let Some((from, to)) = swap {
        script.events.swap(from, to);
    }
    ui.add_enabled_ui(script.events.len() < 4096, |ui| {
        ui.menu_button("Add event", |ui| {
            for (label, event) in [
                ("Text", Step::Input(Input::Text(String::new()))),
                (
                    "Key",
                    Step::Input(Input::Key {
                        code: 28,
                        pressed: true,
                    }),
                ),
                (
                    "Desktop pointer",
                    Step::Input(Input::Pointer {
                        x: 0,
                        y: 0,
                        buttons: 0,
                    }),
                ),
                ("Release held input", Step::Input(Input::ReleaseAll)),
                ("Delay", Step::Wait { millis: 100 }),
                (
                    "Wait for terminal output",
                    Step::WaitPattern {
                        pattern: String::new(),
                        timeout_millis: 5000,
                    },
                ),
                (
                    "Wait for focus",
                    Step::WaitFocus {
                        timeout_millis: 5000,
                    },
                ),
                (
                    "Call macro",
                    Step::Call {
                        name: String::new(),
                    },
                ),
            ] {
                if ui.button(label).clicked() {
                    script.events.push(event);
                    ui.close();
                }
            }
        });
    });
}

fn timeout_control(ui: &mut Ui, millis: &mut u64) {
    ui.label("Timeout");
    ui.add(DragValue::new(millis).range(1..=3_600_000).suffix(" ms"));
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::{captured, frame, owned_workspace, raw_terminal};

    fn value(name: &str, events: Vec<Step>) -> Macro {
        Macro {
            name: name.into(),
            script: Some(Script { version: 1, events }),
            ..Macro::default()
        }
    }

    fn text(value: &str) -> Step {
        Step::Input(Input::Text(value.into()))
    }

    fn wait_for(w: &mut Workspace, ctx: &Context, mut condition: impl FnMut(&Workspace) -> bool) {
        let until = Instant::now() + Duration::from_secs(3);
        while !condition(w) {
            w.pump_terminals();
            w.poll_macros(ctx);
            assert!(Instant::now() < until, "macro fixture did not complete");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn timed_terminal_macro_records_content_and_binds_replay_across_active_switch() {
        let (ctx, _root, mut w) = owned_workspace();
        let first = raw_terminal(&mut w, "first");
        // Diagnostic transport gate: B cannot be acknowledged before ! releases it.
        w.start_with_launch(Session {
            name: "second".into(), protocol: "Local".into(), ..Default::default()
        }, Some(connection::Launch {
            rdp_profile: None, program: "/usr/bin/python3".into(),
            args: vec!["-c".into(), "import os,tty\ntty.setraw(0)\nos.write(1,b'READY')\npending=b''\nwhile True:\n data=os.read(0,32768)\n if not data: break\n for byte in data:\n  if byte==66: pending+=bytes([byte])\n  elif byte==33: os.write(1,b'\\r\\nCAP:'+pending.hex().encode()+b'\\r\\n'); pending=b''\n  else: os.write(1,b'\\r\\nCAP:'+bytes([byte]).hex().encode()+b'\\r\\n')".into()],
            directory: None,
        }));
        wait_for(&mut w, &ctx, |w| w.tabs[1].screen.plain().contains("READY"));
        w.enable_broadcast_all();
        w.tabs[0].send(b"recording barrier");
        wait_for(&mut w, &ctx, |w| {
            captured(&w.tabs[0]) == b"recording barrier"
        });

        w.active = 0;
        w.start_macro_recording(0).unwrap();
        w.set_layout_preset(harbor_workspace::workspace::layout::LayoutPreset::Columns);
        frame(&ctx, &mut w, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(Id::new(("terminal-input", first))));
        frame(&ctx, &mut w, vec![Event::Text("A".into())]);
        std::thread::sleep(Duration::from_millis(30));
        frame(&ctx, &mut w, vec![Event::Text("B".into())]);
        w.finish_macro_recording(0);
        let recorded = w.macro_form.clone();
        let events = &recorded.script.as_ref().unwrap().events;
        assert!(events.iter().any(|event| *event == text("A")), "{events:?}");
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Step::Wait { millis } if *millis >= 20))
        );
        assert_eq!(events.last(), Some(&text("B")));
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]).ends_with(b"AB"));
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[1]) == b"A");
        w.tabs[1].send(b"!");
        wait_for(&mut w, &ctx, |w| {
            captured(&w.tabs[0]).ends_with(b"AB") && captured(&w.tabs[1]) == b"AB"
        });
        let second_at_start = captured(&w.tabs[1]);
        w.queue_macro(first, &recorded).unwrap();
        w.active = 1;
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]).ends_with(b"ABAB"));
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[1]) == b"AB");
        assert_eq!(captured(&w.tabs[1]), second_at_start);
    }

    #[test]
    fn timed_terminal_macro_cancels_with_declined_paste_or_replaced_transport() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "original");
        let multiline = value("confirm", vec![text("A\nB"), text("after")]);
        w.queue_macro(id, &multiline).unwrap();
        w.poll_macros(&ctx);
        assert!(w.pending_paste.is_some());
        assert!(captured(&w.tabs[0]).is_empty());
        w.resolve_paste(false);
        w.poll_macros(&ctx);
        assert!(w.tabs[0].macro_state.playback.is_none());
        assert!(captured(&w.tabs[0]).is_empty());

        let delayed = value(
            "delayed",
            vec![text("A"), Step::Wait { millis: 500 }, text("B")],
        );
        w.queue_macro(id, &delayed).unwrap();
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]) == b"A");
        w.reconnect_tab_now(0);
        assert_ne!(w.tabs[0].id, id);
        let until = Instant::now() + Duration::from_millis(600);
        while Instant::now() < until {
            w.pump_terminals();
            w.poll_macros(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(captured(&w.tabs[0]).is_empty());
    }

    #[test]
    fn timed_macro_waits_for_focus_in_a_detached_terminal_on_another_root_page() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "detached");
        w.detach_terminal(id);
        w.page = Page::Macros;
        w.queue_macro(
            id,
            &value(
                "focus",
                vec![
                    Step::WaitFocus {
                        timeout_millis: 2000,
                    },
                    text("FOCUSED"),
                ],
            ),
        )
        .unwrap();

        let native_id = ViewportId::from_hash_of(("macro-test-child", id));
        let window = w.windows.window(id);
        for focused in [false, true] {
            let mut input = RawInput {
                viewport_id: native_id,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000., 700.))),
                focused,
                ..Default::default()
            };
            input.viewports.insert(
                native_id,
                egui::ViewportInfo {
                    focused: Some(focused),
                    ..Default::default()
                },
            );
            let mut output = ctx.run_ui(input, |ui| {
                ui.ctx()
                    .memory_mut(|memory| memory.request_focus(Id::new(("terminal-input", id))));
                super::super::pages::terminal_view(ui, &mut w.tabs[0], &w.store.preferences);
                w.mark_painted_terminal(ui.ctx(), id, window);
            });
            output.textures_delta.clear();
            // Poll from the root viewport even though the content widget belongs to the child.
            let mut output = ctx.run_ui(
                RawInput {
                    focused: !focused,
                    ..Default::default()
                },
                |ui| {
                    w.poll_macros(ui.ctx());
                },
            );
            output.textures_delta.clear();
            if !focused {
                assert!(w.tabs[0].macro_state.playback.is_some());
            }
        }
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]) == b"FOCUSED");
    }

    #[test]
    fn timed_macro_starts_output_wait_after_the_ninth_input_reaches_the_transport() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "owner");
        let mut events = vec![text("A"); 8];
        events.extend([
            text("Z"),
            Step::WaitPattern {
                pattern: "CAP:41".into(),
                timeout_millis: 2000,
            },
            text("TAIL"),
        ]);
        w.queue_macro(id, &value("boundary", events)).unwrap();
        w.poll_macros(&ctx);
        let until = Instant::now() + Duration::from_secs(2);
        while captured(&w.tabs[0]).len() < 8 {
            w.pump_terminals();
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.poll_macros(&ctx);
        w.poll_macros(&ctx);
        assert!(
            w.tabs[0].macro_state.playback.is_some(),
            "stale output completed the wait"
        );
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]).ends_with(b"Z"));
        w.tabs[0].send_user_bytes(b"A", false).unwrap();
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]).ends_with(b"ZATAIL"));
    }

    #[test]
    fn timed_macro_wait_ignores_pty_output_before_paste_confirmation() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "owner");
        let script = value(
            "paste",
            vec![
                text(
                    "Z
",
                ),
                Step::WaitPattern {
                    pattern: "CAP:41".into(),
                    timeout_millis: 2000,
                },
                text("TAIL"),
            ],
        );
        w.queue_macro(id, &script).unwrap();
        w.poll_macros(&ctx);
        assert!(w.pending_paste.is_some());
        w.tabs[0].send_user_bytes(b"A", false).unwrap();
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]) == b"A");
        w.resolve_paste(true);
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]).ends_with(b"[201~"));
        assert!(
            w.tabs[0].macro_state.playback.is_some(),
            "pre-confirmation output completed the wait"
        );
        w.tabs[0].send_user_bytes(b"A", false).unwrap();
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]).ends_with(b"ATAIL"));
    }

    #[test]
    fn timed_macro_startup_sees_the_first_authenticated_ssh_output() {
        use harbor_workspace::ssh::ConnectionEvent;
        let (ctx, root, mut w) = owned_workspace();
        let server = super::ssh_fixture::SshServer::start(false);
        let mut config = server.config(1, true);
        let known_hosts = root.path().join("ssh/known_hosts");
        fs::create_dir_all(known_hosts.parent().unwrap()).unwrap();
        fs::copy(&config.app_known_hosts, known_hosts).unwrap();
        let release = server.root.join("release-startup-prompt");
        let remote_command = server.root.join("startup-prompt.sh");
        fs::write(
            &remote_command,
            r#"stty raw -echo
until [ -f "${0%/*}/release-startup-prompt" ]; do sleep 0.01; done
printf FIRST_SSH_PROMPT
exec cat
"#,
        )
        .unwrap();
        config.session.command = format!("exec /bin/sh '{}'", remote_command.display());
        config.session.startup_macro = Some("banner".into());
        w.store.macros.push(value(
            "banner",
            vec![
                Step::WaitPattern {
                    pattern: "FIRST_SSH_PROMPT".into(),
                    timeout_millis: 1000,
                },
                text("STARTUP_REPLIED"),
            ],
        ));
        w.start(config.session);
        assert!(!w.tabs[0].ssh_connected);
        let terminal = w.tabs[0].id;
        w.tabs[0].poll_with_ssh_batch(super::super::terminal_output::TICK_BYTES, |ssh, budget| {
            // Collect genuine worker events without letting the tab act on Connected
            // until the same batch also contains the initial authenticated output.
            const PROMPT: &[u8] = b"FIRST_SSH_PROMPT";
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut batch = Vec::new();
            let mut output = Vec::new();
            let mut connected = false;
            loop {
                let events = ssh.poll_with_output_budget(budget.saturating_sub(output.len()));
                for event in &events {
                    match event {
                        ConnectionEvent::Connected { connection } if *connection == terminal => {
                            assert!(!connected, "SSH published Connected more than once");
                            connected = true;
                            fs::write(&release, b"").unwrap();
                        }
                        ConnectionEvent::Output { connection, bytes }
                            if *connection == terminal =>
                        {
                            assert!(connected, "shell output preceded authentication");
                            output.extend_from_slice(bytes);
                        }
                        ConnectionEvent::Challenge { title, .. } => {
                            panic!("unexpected SSH authentication challenge: {title}");
                        }
                        ConnectionEvent::Exited { message, .. } => {
                            panic!("SSH exited before delivering its prompt: {message}");
                        }
                        _ => {}
                    }
                }
                batch.extend(events);
                if output.windows(PROMPT.len()).any(|window| window == PROMPT) {
                    return batch;
                }
                assert!(
                    output.len() < budget,
                    "SSH fixture output exceeded one app poll"
                );
                assert!(
                    Instant::now() < deadline,
                    "SSH peer did not deliver its released prompt"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        assert!(release.exists());
        assert!(w.tabs[0].ssh_connected, "{}", w.tabs[0].error);
        assert!(w.tabs[0].screen.plain().contains("FIRST_SSH_PROMPT"));
        wait_for(&mut w, &ctx, |w| {
            w.tabs[0].screen.plain().contains("STARTUP_REPLIED")
        });
    }

    #[test]
    fn timed_macro_waits_for_its_pty_output_and_calls_the_named_child() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "owner");
        raw_terminal(&mut w, "other");
        w.store.macros.push(value("child", vec![text("B")]));
        let script = value(
            "output",
            vec![
                text("A"),
                Step::WaitPattern {
                    pattern: "CAP:41".into(),
                    timeout_millis: 1000,
                },
                Step::Call {
                    name: "child".into(),
                },
            ],
        );
        w.queue_macro(id, &script).unwrap();
        wait_for(&mut w, &ctx, |w| captured(&w.tabs[0]) == b"AB");
        assert!(captured(&w.tabs[1]).is_empty());
    }

    #[test]
    fn macro_import_is_bounded_and_rejects_invalid_call_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("macros.json");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_IMPORT_BYTES + 1)
            .unwrap();
        assert!(read_import(&path).unwrap_err().contains("4 MiB"));
        fs::write(
            &path,
            r#"[{"name":"legacy","text":"ls","append_enter":false}]"#,
        )
        .unwrap();
        let legacy = read_import(&path).unwrap();
        assert!(legacy[0].script.is_none());
        validate_library(&legacy).unwrap();
        assert!(
            validate_library(&[value(
                "caller",
                vec![Step::Call {
                    name: "missing".into()
                }]
            )])
            .is_err()
        );
        assert!(
            validate_library(&[value(
                "cycle",
                vec![Step::Call {
                    name: "cycle".into()
                }]
            )])
            .is_err()
        );
    }

    #[test]
    fn timed_macro_never_records_unauthenticated_terminal_input() {
        let (ctx, _root, mut w) = owned_workspace();
        w.start(Session {
            host: "127.0.0.1".into(),
            port: 1,
            ..Session::default()
        });
        assert!(w.start_macro_recording(0).is_err());
        let target = w.tabs[0].macro_target();
        w.tabs[0].macro_state.capture.start(target);
        w.tabs[0].send(b"SECRET");
        w.poll_macros(&ctx);
        assert!(!w.tabs[0].macro_state.capture.active());
        w.finish_macro_recording(0);
        assert!(w.macro_form.script.as_ref().unwrap().events.is_empty());
    }

    #[test]
    fn timed_macro_startup_survives_duplication_and_named_cli_runs_once() {
        let (ctx, _root, mut w) = owned_workspace();
        let startup = "startup";
        w.store
            .macros
            .push(value(startup, vec![text("echo STARTUP\n")]));
        w.store.macros.push(value(
            "named",
            vec![Step::Call {
                name: startup.into(),
            }],
        ));
        let original = Session {
            name: "original".into(),
            protocol: "Local".into(),
            startup_macro: Some(startup.into()),
            ..Session::default()
        };
        w.store.sessions.push(original);
        w.duplicate_saved_session(0);
        let copy = w.store.sessions[1].clone();
        assert_ne!(copy.id, w.store.sessions[0].id);
        assert_eq!(copy.startup_macro.as_deref(), Some(startup));
        let id = w.launch_saved_session(copy.clone()).unwrap();
        wait_for(&mut w, &ctx, |w| w.pending_paste.is_some());
        w.resolve_paste(true);
        wait_for(&mut w, &ctx, |w| {
            w.tabs[0].screen.plain().contains("\nSTARTUP\n")
        });
        w.dispatch_action(
            harbor_workspace::cli::CliAction::RunMacro {
                name: "named".into(),
                session: Some(copy.id),
                cwd: None,
                exit_when_done: false,
            },
            &ctx,
        )
        .unwrap();
        assert_ne!(w.tabs[1].id, id);
        wait_for(&mut w, &ctx, |w| w.pending_paste.is_some());
        w.resolve_paste(true);
        wait_for(&mut w, &ctx, |w| {
            w.tabs[1].screen.plain().contains("\nSTARTUP\n")
        });
        assert!(w.tabs[1].macro_state.pending.is_none());
        assert!(w.pending_paste.is_none());
    }
}

#[cfg(all(test, unix))]
#[allow(dead_code)]
#[path = "../../tests/support/ssh_server.rs"]
mod ssh_fixture;
