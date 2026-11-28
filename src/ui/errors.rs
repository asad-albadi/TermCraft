//! Compact, shared error presentation. Codes open safe details and explicit copy actions.
use super::*;
pub(super) use harbor_workspace::diagnostics::Context as Domain;
use harbor_workspace::diagnostics::Diagnostic;

pub(super) fn protocol(protocol: &str) -> Domain {
    match protocol {
        "SSH" | "Mosh" => Domain::Ssh,
        "SFTP" | "FTP" => Domain::Transfer,
        "RDP" => Domain::Rdp,
        "VNC" => Domain::Vnc,
        "XDMCP" => Domain::X11,
        "Browser" => Domain::Browser,
        _ => Domain::Session,
    }
}
pub(super) fn show(ui: &mut Ui, domain: Domain, message: impl AsRef<str>) -> Response {
    draw(ui, domain, message.as_ref(), false, false)
}
pub(super) fn compact(ui: &mut Ui, domain: Domain, message: &str) -> Response {
    draw(ui, domain, message, true, false)
}
pub(super) fn warning(ui: &mut Ui, domain: Domain, message: impl AsRef<str>) -> Response {
    draw(ui, domain, message.as_ref(), false, true)
}
pub(super) fn cancelled(message: &str) -> bool {
    matches!(
        message
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase()
            .as_str(),
        "cancelled"
            | "canceled"
            | "operation cancelled"
            | "operation canceled"
            | "connection cancelled"
            | "connection canceled"
    )
}
fn draw(ui: &mut Ui, domain: Domain, message: &str, compact: bool, warning: bool) -> Response {
    if cancelled(message) {
        return ui.label(harbor_workspace::diagnostics::sanitize(message));
    }
    let diagnostic = Diagnostic::new(domain, message);
    ui.push_id(("diagnostic", domain.prefix(), &diagnostic.details), |ui| {
        let row = |ui: &mut Ui| {
            let palette = fluent::palette(ui);
            let color = if warning {
                ui.visuals().warn_fg_color
            } else {
                palette.error
            };
            let details = ui
                .menu_button(
                    RichText::new(&diagnostic.code)
                        .monospace()
                        .size(12.)
                        .color(color),
                    |ui| {
                        ui.set_max_width((ui.ctx().content_rect().width() - 40.).clamp(1., 500.));
                        ui.label(fluent::emphasized(
                            ui.ctx(),
                            RichText::new(&diagnostic.summary),
                        ));
                        ui.label(&diagnostic.next_step);
                        ui.separator();
                        ui.weak("Technical details");
                        ScrollArea::vertical().max_height(180.).show(ui, |ui| {
                            ui.label(RichText::new(&diagnostic.details).monospace());
                        });
                        if ui.button("Copy error details").clicked() {
                            ui.ctx().copy_text(diagnostic.report());
                            ui.close();
                        }
                    },
                )
                .response;
            details.fluent_hint("Open error details and suggested next steps");
            let label = Label::new(RichText::new(&diagnostic.summary).color(color));
            ui.add(if compact {
                label.truncate()
            } else {
                label.wrap()
            })
            .fluent_hint(&diagnostic.next_step)
        };
        if compact {
            ui.horizontal(row).inner
        } else {
            ui.horizontal_wrapped(row).inner
        }
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::gui_tests::{shell_frame, shell_pointer, shell_text_position};
    #[test]
    fn error_code_opens_details_and_copy_is_explicit_and_redacted() {
        let (ctx, _root, _workspace) = terminal_review_tests::owned_workspace();
        let message = "Password authentication failed; password=DO_NOT_COPY_THIS";
        let draw = |events| {
            shell_frame(&ctx, vec2(640., 480.), 1., events, |ui| {
                show(ui, Domain::Ssh, message);
            })
        };
        draw(vec![]);
        let first = draw(vec![]);
        assert!(!crate::ui::pane_cleanup_tests::labels(&first).contains("DO_NOT_COPY_THIS"));
        assert!(
            !first
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
        );
        let point = shell_text_position(&first, "TC-SSH-104");
        draw(vec![Event::PointerMoved(point), shell_pointer(point, true)]);
        draw(vec![shell_pointer(point, false)]);
        draw(vec![]);
        let popup = draw(vec![]);
        assert!(
            !popup
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
        );
        let copy = shell_text_position(&popup, "Copy error details");
        draw(vec![Event::PointerMoved(copy), shell_pointer(copy, true)]);
        let result = draw(vec![shell_pointer(copy, false)]);
        let copied = result
            .platform_output
            .commands
            .iter()
            .find_map(|c| {
                if let egui::OutputCommand::CopyText(text) = c {
                    Some(text)
                } else {
                    None
                }
            })
            .expect("Explicit copy did not produce a report");
        assert!(
            copied.contains("TC-SSH-104")
                && copied.contains("Next step:")
                && copied.contains("[redacted]")
        );
        assert!(!copied.contains("DO_NOT_COPY_THIS"));
        assert!(
            message.contains("DO_NOT_COPY_THIS"),
            "Presentation must not rewrite source state"
        );
    }
    #[test]
    fn typed_error_status_is_distinct_from_cancellation_and_success() {
        let (ctx, _root, mut w) = terminal_review_tests::owned_workspace();
        w.error_in(Domain::Files, "permission denied");
        assert!(w.status_error);
        let draw =
            |w: &mut Workspace| shell_frame(&ctx, vec2(640., 480.), 1., vec![], |ui| w.top(ui));
        draw(&mut w);
        let output = draw(&mut w);
        assert!(crate::ui::pane_cleanup_tests::labels(&output).contains("TC-FILES-105"));
        w.message("Shortcut creation canceled.");
        assert!(!w.status_error);
        w.error_in(Domain::Files, "Operation cancelled.");
        assert!(
            !w.status_error,
            "A reported cancellation is not a failure notification"
        );
        let cancelled = draw(&mut w);
        assert!(!crate::ui::pane_cleanup_tests::labels(&cancelled).contains("TC-FILES-105"));
    }
}

#[cfg(all(test, unix))]
mod connection_tests {
    use super::*;
    #[test]
    fn connection_refusal_cause_survives_automatic_terminal_resize() {
        let (ctx, _root, mut w) = terminal_review_tests::owned_workspace();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        w.start(Session {
            host: "127.0.0.1".into(),
            port,
            ..Default::default()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            w.tabs[0].poll();
            if w.tabs[0].ssh.as_ref().unwrap().is_finished() && !w.tabs[0].error.is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Owned refused connection did not report its cause"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        w.tabs[0].poll();
        let cause = w.tabs[0].error.clone();
        assert_eq!(
            harbor_workspace::diagnostics::Diagnostic::new(Domain::Ssh, &cause).code,
            "TC-SSH-101",
            "Unexpected owned loopback error: {cause}"
        );
        w.tabs[0].screen.resize(91, 27);
        w.tabs[0].resize_transport();
        assert_eq!(
            w.tabs[0].error, cause,
            "A resize after worker exit hid the connection cause"
        );
        assert_eq!(w.tabs[0].connection_status, "Disconnected");
        let mut output = ctx.run_ui(Default::default(), |ui| w.terminals(ui));
        output.textures_delta.clear();
        assert!(pane_cleanup_tests::labels(&output).contains("TC-SSH-101"));
        let id = w.tabs[0].id;
        w.tabs[0].poll_with_ssh_batch(0, |_, _| {
            vec![harbor_workspace::ssh::ConnectionEvent::Exited {
                connection: id,
                message: "Process ended with status 0".into(),
            }]
        });
        assert!(
            w.tabs[0].error.is_empty(),
            "A graceful remote exit is not an error"
        );
        assert_eq!(w.tabs[0].connection_status, "Process ended with status 0");
    }
}
