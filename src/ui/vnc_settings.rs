use super::*;
pub(super) fn session(ui: &mut Ui, s: &mut Session) {
    use harbor_workspace::desktop::{
        rdp_options::{ClipboardPolicy, KeyboardRouting},
        vnc::Security,
    };
    ui.checkbox(&mut s.vnc_options.embedded, "Embed VNC in TermCraft");
    if s.vnc_options.embedded {
        ui.horizontal_wrapped(|ui| {
            ui.label("Security");
            for (security, label) in [
                (Security::TlsPassword, "Verified TLS + password"),
                (Security::TlsNone, "Verified TLS"),
                (Security::Password, "Classic password"),
                (Security::None, "Classic no authentication"),
            ] {
                ui.selectable_value(&mut s.vnc_options.security, security, label);
            }
        });
        if s.vnc_options.security.encrypted() {
            ui.small("VeNCrypt verifies the server certificate and hostname before authentication. Anonymous TLS and security downgrades are refused.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Certificate authority (optional)");
                if ui.button("Choose PEM…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        s.vnc_options.certificate_authority = Some(path);
                    }
                }
                if s.vnc_options.certificate_authority.is_some()
                    && ui.button("Use system trust").clicked()
                {
                    s.vnc_options.certificate_authority = None;
                }
            });
            if let Some(path) = &s.vnc_options.certificate_authority {
                ui.label(path.display().to_string());
            }
        } else {
            ui.checkbox(
                &mut s.vnc_options.allow_unencrypted,
                "Allow classic unencrypted VNC on this trusted network",
            );
            ui.small("Classic VNC does not verify server identity or encrypt desktop traffic. The optional CA applies only to verified TLS mode.");
        }
        ui.small("SSH gateways and Unicode clipboard extensions remain unavailable.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Text clipboard (Latin-1)");
            for (policy, label) in [
                (ClipboardPolicy::Disabled, "Disabled"),
                (ClipboardPolicy::ToRemote, "To remote"),
                (ClipboardPolicy::FromRemote, "From remote"),
                (ClipboardPolicy::Bidirectional, "Both directions"),
            ] {
                ui.selectable_value(&mut s.vnc_options.clipboard, policy, label);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Keyboard");
            ui.selectable_value(
                &mut s.vnc_options.keyboard,
                KeyboardRouting::Remote,
                "Remote",
            );
            ui.selectable_value(&mut s.vnc_options.keyboard, KeyboardRouting::Local, "Local");
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn label(shape: &egui::epaint::Shape, value: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == value => {
                Some(t.pos + t.galley.size() / 2.)
            }
            egui::epaint::Shape::Vec(items) => items.iter().find_map(|s| label(s, value)),
            _ => None,
        }
    }
    #[test]
    fn vnc_controls_require_consent_and_keep_clipboard_and_keyboard_choices() {
        let ctx = Context::default();
        let mut s = Session {
            protocol: "VNC".into(),
            ..Default::default()
        };
        for target in [
            "Embed VNC in TermCraft",
            "Classic password",
            "Allow classic unencrypted VNC on this trusted network",
            "From remote",
            "Local",
        ] {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 800.))),
                    ..Default::default()
                },
                |ui| session(ui, &mut s),
            );
            let point = output
                .shapes
                .iter()
                .find_map(|item| label(&item.shape, target))
                .unwrap_or_else(|| panic!("missing {target}"));
            output.textures_delta.clear();
            for pressed in [true, false] {
                let mut clicked = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 800.))),
                        events: vec![
                            Event::PointerMoved(point),
                            Event::PointerButton {
                                pos: point,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::default(),
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| session(ui, &mut s),
                );
                clicked.textures_delta.clear();
            }
        }
        assert!(s.vnc_options.embedded && s.vnc_options.allow_unencrypted);
        assert_eq!(
            s.vnc_options.clipboard,
            harbor_workspace::desktop::rdp_options::ClipboardPolicy::FromRemote
        );
        assert_eq!(
            s.vnc_options.keyboard,
            harbor_workspace::desktop::rdp_options::KeyboardRouting::Local
        );
        let decoded: Session = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(decoded.vnc_options, s.vnc_options);
    }
}
