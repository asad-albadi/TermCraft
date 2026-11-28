use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::{captured, owned_workspace, raw_terminal};
    use harbor_workspace::{
        cli::CliAction,
        ipc::{Binding, InstanceBroker},
    };
    #[test]
    fn forwarded_new_tab_opens_once_in_the_actual_workspace() {
        let (ctx, root, mut w) = owned_workspace();
        let Binding::Primary(primary) = InstanceBroker::bind(root.path()).unwrap() else {
            panic!()
        };
        w.attach_dispatch(primary, CliAction::Activate);
        let Binding::Secondary(client) = InstanceBroker::bind(root.path()).unwrap() else {
            panic!()
        };
        client
            .send_with_id(
                "ui-owned",
                CliAction::NewTab {
                    cwd: Some(root.path().to_owned()),
                    exit_when_done: false,
                },
            )
            .unwrap();
        client
            .send_with_id(
                "ui-owned",
                CliAction::NewTab {
                    cwd: Some(root.path().to_owned()),
                    exit_when_done: false,
                },
            )
            .unwrap();
        w.poll_dispatch(&ctx);
        w.poll_dispatch(&ctx);
        assert_eq!(w.tabs.len(), 1);
        assert!(w.tabs[0].process.is_some());
    }
    #[test]
    fn command_dispatch_uses_its_own_process_and_exact_local_cwd() {
        let (ctx, root, mut w) = owned_workspace();
        raw_terminal(&mut w, "existing");
        w.enable_broadcast_all();
        let dir = root.path().join("directory with \" quotes");
        fs::create_dir(&dir).unwrap();
        w.dispatch_action(
            CliAction::RunCommand {
                command: "printf 'CLI_SENTINEL\\n'; pwd".into(),
                session: None,
                cwd: Some(dir.clone()),
                exit_when_done: false,
            },
            &ctx,
        )
        .unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            for t in &mut w.tabs {
                t.poll();
            }
            if w.tabs[1].screen.plain().contains(dir.to_str().unwrap()) {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.tabs[1].screen.plain().contains("CLI_SENTINEL"));
        assert!(captured(&w.tabs[0]).is_empty());
    }
    #[test]
    fn named_macro_targets_its_new_terminal_and_respects_paste_confirmation() {
        let (ctx, _root, mut w) = owned_workspace();
        raw_terminal(&mut w, "existing");
        w.enable_broadcast_all();
        w.store.macros.push(Macro {
            script: None,
            name: "owned".into(),
            text: "printf '\\115\\101\\103\\122\\117\\137\\122\\105\\101\\104\\131\\n'".into(),
            append_enter: true,
        });
        w.dispatch_action(
            CliAction::RunMacro {
                name: "owned".into(),
                session: None,
                cwd: None,
                exit_when_done: false,
            },
            &ctx,
        )
        .unwrap();
        let end = Instant::now() + Duration::from_secs(4);
        loop {
            w.poll_dispatch(&ctx);
            if w.pending_paste.is_some() {
                w.resolve_paste(true);
            }
            for t in &mut w.tabs {
                t.tick_paste(Instant::now());
                t.poll();
            }
            if w.tabs[1].screen.plain().contains("MACRO_READY") {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(captured(&w.tabs[0]).is_empty());
    }
    #[test]
    fn exit_when_done_retires_telnet_job_when_gateway_cannot_connect() {
        let (ctx, _root, mut workspace) = owned_workspace();
        let original = raw_terminal(&mut workspace, "keep");
        let gateway = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        gateway.set_nonblocking(true).unwrap();
        let session = Session {
            protocol: "Telnet".into(),
            host: "private-target.invalid".into(),
            port: 23,
            jump: format!("gateway-user@{}", gateway.local_addr().unwrap()),
            ..Default::default()
        };
        let id = session.id.clone();
        workspace.store.sessions.push(session);
        workspace
            .dispatch_action(
                CliAction::OpenSession {
                    id,
                    cwd: None,
                    exit_when_done: true,
                },
                &ctx,
            )
            .unwrap();
        assert_eq!(workspace.tabs.len(), 2);
        let job = workspace.tabs[1].id;
        assert_eq!(workspace.dispatch.exit_tabs, [job]);

        let end = Instant::now() + Duration::from_secs(3);
        while workspace.tabs[1].error.is_empty() {
            // Close the fixture connection before SSH identification. A bound
            // but non-listening socket need not yield immediate refusal on macOS.
            match gateway.accept() {
                Ok((stream, _)) => drop(stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("could not accept fixture gateway connection: {error}"),
            }
            workspace.tabs[1].poll();
            assert!(
                Instant::now() < end,
                "gateway failure did not reach the tab"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(workspace.tabs[1].connection_status.contains("gateway"));
        assert!(workspace.tabs[1].legacy.is_none());
        assert!(workspace.tabs[1].gateway.is_none());

        workspace.poll_dispatch(&ctx);
        assert!(
            workspace.dispatch.exit_tabs.is_empty(),
            "finished CLI job stayed pending"
        );
        assert_eq!(workspace.tabs.len(), 1);
        assert_eq!(workspace.tabs[0].id, original);
        assert!(workspace.tabs[0].running());
        assert!(!workspace.dispatch.close_when_empty);
    }

    #[test]
    fn exit_when_done_preserves_preexisting_user_terminals() {
        let (ctx, _root, mut w) = owned_workspace();
        let original = raw_terminal(&mut w, "keep");
        w.dispatch_action(
            CliAction::RunCommand {
                command: "exit 0".into(),
                session: None,
                cwd: None,
                exit_when_done: true,
            },
            &ctx,
        )
        .unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while w.tabs.len() > 1 {
            for tab in &mut w.tabs {
                tab.poll();
            }
            w.poll_dispatch(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(w.tabs[0].id, original);
        assert!(w.tabs[0].running());
        assert!(!w.dispatch.close_when_empty);
    }
}
use harbor_workspace::{
    cli::CliAction,
    ipc::{DispatchRequest, Primary},
};
use std::collections::VecDeque;
#[derive(Default)]
pub(super) struct State {
    primary: Option<Primary>,
    pending: VecDeque<DispatchRequest>,
    exit_tabs: Vec<u64>,
    close_when_empty: bool,
}
impl Workspace {
    pub(crate) fn attach_dispatch(&mut self, primary: Primary, initial: CliAction) {
        self.dispatch.primary = Some(primary);
        self.dispatch.pending.push_back(DispatchRequest {
            id: "startup".into(),
            action: initial,
        });
    }
    pub(super) fn poll_dispatch(&mut self, ctx: &Context) {
        if self.dispatch.primary.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.import_applying() {
            return;
        }
        if let Some(primary) = &mut self.dispatch.primary {
            while self.dispatch.pending.len() < 32 {
                match primary.try_recv() {
                    Ok(request) => self.dispatch.pending.push_back(request),
                    Err(_) => break,
                }
            }
        }
        for _ in 0..4 {
            let Some(request) = self.dispatch.pending.pop_front() else {
                break;
            };
            if let Err(error) = self.dispatch_action(request.action, ctx) {
                self.error_in(
                    errors::Domain::CommandLine,
                    format!("Launch request {} failed: {error}", request.id),
                );
            }
        }
        self.poll_macros(ctx);
        if self.dispatch.close_when_empty
            && self
                .tabs
                .iter()
                .any(|t| !self.dispatch.exit_tabs.contains(&t.id))
        {
            self.dispatch.close_when_empty = false;
        }
        for id in std::mem::take(&mut self.dispatch.exit_tabs) {
            let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
                continue;
            };
            let tab = &self.tabs[index];
            let failed_telnet_gateway = tab.session.protocol == "Telnet"
                && !tab.session.jump.trim().is_empty()
                && tab.gateway.is_none()
                && tab.legacy.is_none()
                && tab.output_ended
                && tab.input_failed;
            let finished = failed_telnet_gateway
                || tab.process.as_ref().is_some_and(|p| p.exited)
                || tab.ssh.as_ref().is_some_and(|s| s.is_finished())
                || tab.serial.is_some() && tab.output_ended
                || tab
                    .legacy
                    .as_ref()
                    .is_some_and(|legacy| legacy.is_finished())
                    && tab.output_ended;
            if finished {
                self.close_tab(index);
            } else {
                self.dispatch.exit_tabs.push(id)
            }
        }
        if self.dispatch.close_when_empty
            && self.tabs.is_empty()
            && self.dispatch.pending.is_empty()
            && self.pending_session_close.is_none()
        {
            self.dispatch.close_when_empty = false;
            self.request_window_session_close(ctx);
        }
    }
    fn cli_session(&self, id: Option<&str>, cwd: Option<PathBuf>) -> Result<Session, String> {
        let mut session = if let Some(id) = id {
            let mut matches = self.store.sessions.iter().filter(|s| s.id == id);
            let s = matches
                .next()
                .ok_or("Saved session ID was not found in this workspace.")?
                .clone();
            if matches.next().is_some() {
                return Err("Saved session ID is ambiguous.".into());
            }
            s
        } else {
            Session {
                protocol: "Local".into(),
                name: "Command line".into(),
                ..Default::default()
            }
        };
        if let Some(path) = cwd {
            if session.protocol != "Local" {
                return Err("--cwd applies to local terminals; use the remote session's command for remote working directories.".into());
            }
            if !path.is_dir() {
                return Err("Starting directory does not exist or is unavailable.".into());
            }
            session.directory = path
                .to_str()
                .ok_or("Starting directory requires valid Unicode")?
                .into();
        }
        Ok(session)
    }
    fn track_cli_exit(&mut self, id: u64, exit: bool, was_empty: bool) {
        if exit {
            self.dispatch.exit_tabs.push(id);
            if was_empty {
                self.dispatch.close_when_empty = true;
            }
        }
    }
    pub(super) fn dispatch_action(
        &mut self,
        action: CliAction,
        ctx: &Context,
    ) -> Result<(), String> {
        if self.import_applying() {
            return Err(
                "Session import is committing; dispatch remains queued until it finishes.".into(),
            );
        }
        action.validate()?;
        let was_empty = self.tabs.is_empty();
        match action {
            CliAction::Activate => {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            CliAction::Help | CliAction::Screenshot { .. } => {
                return Err("This action needs a separate process.".into());
            }
            CliAction::NewTab {
                cwd,
                exit_when_done,
            } => {
                let session = self.cli_session(None, cwd)?;
                let id = self.launch_saved_session(session)?;
                self.track_cli_exit(id, exit_when_done, was_empty);
            }
            CliAction::OpenFolder {
                path,
                exit_when_done,
            } => {
                let session = self.cli_session(None, Some(path))?;
                let id = self.launch_saved_session(session)?;
                self.track_cli_exit(id, exit_when_done, was_empty);
            }
            CliAction::OpenSession {
                id,
                cwd,
                exit_when_done,
            } => {
                let session = self.cli_session(Some(&id), cwd)?;
                let id = self.launch_saved_session(session)?;
                self.track_cli_exit(id, exit_when_done, was_empty);
            }
            CliAction::OpenGroup {
                folder,
                cwd,
                exit_when_done,
            } => {
                use harbor_workspace::sessions::groups::{GroupFailure, GroupReport, SessionGroup};
                let sessions = SessionGroup::resolve(&folder, &self.store);
                if sessions.is_empty() {
                    return Err("Saved group was not found in this workspace.".into());
                }
                let mut report = GroupReport::default();
                for session in sessions {
                    let id = session.id.clone();
                    let name = session.title();
                    let result = self
                        .cli_session(Some(&id), cwd.clone())
                        .and_then(|s| self.launch_saved_session(s));
                    match result {
                        Ok(terminal) => {
                            report.accepted.push(id);
                            self.track_cli_exit(terminal, exit_when_done, was_empty);
                        }
                        Err(error) => report.failed.push(GroupFailure { id, name, error }),
                    }
                }
                self.session_groups.report = Some(report);
                self.session_groups.folder = folder;
            }
            CliAction::RunCommand {
                command,
                session,
                cwd,
                exit_when_done,
            } => {
                let mut session = self.cli_session(session.as_deref(), cwd)?;
                let id = if session.protocol == "SSH" {
                    session.command = command;
                    self.launch_saved_session(session)?
                } else if session.protocol == "Local" {
                    if self.tabs.len() >= 64 {
                        return Err("64 terminal tabs are already open.".into());
                    }
                    let mut launch = connection::launch(&session, &self.store.preferences)?;
                    let program = launch.program.to_lowercase();
                    launch.args = if cfg!(windows) {
                        if program.ends_with("cmd.exe") || program.ends_with("cmd") {
                            vec!["/D".into(), "/C".into(), command]
                        } else if program.contains("powershell")
                            || program.ends_with("pwsh")
                            || program.ends_with("pwsh.exe")
                        {
                            vec![
                                "-NoLogo".into(),
                                "-NoProfile".into(),
                                "-Command".into(),
                                command,
                            ]
                        } else {
                            vec!["-c".into(), command]
                        }
                    } else {
                        vec!["-c".into(), command]
                    };
                    self.start_with_launch(session, Some(launch));
                    let index = self.active;
                    if !self.tabs[index].error.is_empty() {
                        let error = self.tabs[index].error.clone();
                        self.force_close_tab(index);
                        return Err(error);
                    }
                    self.tabs[index].id
                } else {
                    return Err("Commands require a Local or SSH session.".into());
                };
                self.track_cli_exit(id, exit_when_done, was_empty);
            }
            CliAction::RunMacro {
                name,
                session,
                cwd,
                exit_when_done,
            } => {
                let mut session = self.cli_session(session.as_deref(), cwd)?;
                if !super::macro_events::supported_session(&session) {
                    return Err("Macros require an owned terminal or embedded desktop.".into());
                }
                if self
                    .store
                    .macros
                    .iter()
                    .filter(|value| value.name == name)
                    .count()
                    != 1
                {
                    return Err("Saved macro is missing or its name is ambiguous.".into());
                }
                let attached = session.startup_macro.take();
                let id = self.launch_saved_session(session)?;
                self.tabs[self.active].session.startup_macro = attached;
                if let Err(error) = self.queue_named_macro(id, &name) {
                    self.force_close_tab(self.active);
                    return Err(error);
                }
                self.track_cli_exit(id, exit_when_done, was_empty);
            }
        }
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        Ok(())
    }
}
