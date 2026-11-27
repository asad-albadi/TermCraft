use super::*;
pub(super) fn session(ui: &mut Ui, session: &mut Session) {
    use harbor_workspace::desktop::rdp_options::*;
    let embedded = session.desktop_engine == harbor_workspace::model::DesktopEngine::Embedded;
    ui.separator();
    ui.label(RichText::new("RDP settings").strong());
    let options = &mut session.rdp_options;
    ui.horizontal(|ui| {
        ui.label("Domain");
        ui.text_edit_singleline(&mut options.domain);
    });
    ui.label("Clipboard · text only, maximum 8192 UTF-8 bytes");
    ui.horizontal_wrapped(|ui| {
        for (policy, label) in [
            (ClipboardPolicy::Disabled, "Disabled"),
            (ClipboardPolicy::ToRemote, "To remote"),
            (ClipboardPolicy::FromRemote, "From remote"),
            (ClipboardPolicy::Bidirectional, "Both directions"),
        ] {
            ui.selectable_value(&mut options.clipboard, policy, label);
        }
    });
    ui.small("Remote text is copied locally only when you choose Copy remote clipboard. Paste sends clipboard text through the negotiated channel.");
    ui.horizontal_wrapped(|ui| {
        ui.label("Keyboard");
        ui.selectable_value(
            &mut options.keyboard,
            KeyboardRouting::Remote,
            "Send keyboard remotely",
        );
        ui.selectable_value(
            &mut options.keyboard,
            KeyboardRouting::Local,
            "Keep keyboard local",
        );
    });
    ui.horizontal(|ui| {
        ui.label("Keyboard layout ID (0 = engine default)");
        ui.add(DragValue::new(&mut options.keyboard_layout));
    });
    if embedded {
        ui.label("Security · verified TLS with NLA");
        ui.small("Legacy security and TLS graphical sign-in have no embedded provider.");
        ui.horizontal(|ui| {
            ui.label("Certificate authority (optional)");
            if ui.button("Choose PEM…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    options.certificate_authority = Some(path);
                }
            }
            if options.certificate_authority.is_some() && ui.button("Use system trust").clicked() {
                options.certificate_authority = None;
            }
        });
        if let Some(path) = &options.certificate_authority {
            ui.label(path.display().to_string());
        }
    }
    ui.checkbox(&mut options.administrative, "Administrative session");
    {
        ui.label(RichText::new("Redirected folders").strong());
        ui.small("Only the folders listed here are shared. New mappings are read only. Writable mappings allow file creation, overwrites and resizing. Linux supports allocation requests and releases unused reservations on close. Rename, deletion and non-Linux allocation remain unavailable; applications that save by renaming a temporary file cannot use these mappings.");
        let mut remove = None;
        for (index, drive) in options.drives.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Name (1–7 characters)");
                    ui.add(TextEdit::singleline(&mut drive.name).desired_width(90.));
                    ui.checkbox(&mut drive.read_only, "Read only");
                    if ui.button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
                ui.horizontal(|ui| {
                    let mut path = drive.path.display().to_string();
                    if ui
                        .add(
                            TextEdit::singleline(&mut path)
                                .hint_text("Choose an absolute local folder"),
                        )
                        .changed()
                    {
                        drive.path = path.into();
                    }
                    if ui.button("Choose folder…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            drive.path = path;
                        }
                    }
                });
            });
        }
        if let Some(index) = remove {
            options.drives.remove(index);
        }
        if ui
            .add_enabled(
                embedded && options.drives.len() < 16,
                Button::new("Add folder mapping"),
            )
            .clicked()
        {
            let name = (1..=17)
                .map(|n| format!("Drive{n}"))
                .find(|name| {
                    !options
                        .drives
                        .iter()
                        .any(|drive| drive.name.eq_ignore_ascii_case(name))
                })
                .unwrap();
            options.drives.push(DriveMapping {
                name,
                path: Default::default(),
                read_only: true,
            });
        }
    }
    ui.label(RichText::new("Explicit serial mappings").strong());
    ui.small("Share only explicitly named local serial devices. Read-only mappings refuse remote writes. Native device compatibility still requires validation.");
    let mut remove_serial = None;
    for (index, mapping) in options.serial_mappings.iter_mut().enumerate() {
        ui.push_id(("rdp-serial-mapping", index), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Port name");
                ui.add(TextEdit::singleline(&mut mapping.name).desired_width(80.));
                ui.checkbox(&mut mapping.read_only, "Read only");
                if ui.button("Remove serial mapping").clicked() {
                    remove_serial = Some(index);
                }
            });
            let mut path = mapping.path.display().to_string();
            if ui
                .add(TextEdit::singleline(&mut path).hint_text(if cfg!(windows) {
                    r"Windows device path, for example \\.\COM3"
                } else {
                    "Absolute local device path, for example /dev/ttyUSB0"
                }))
                .changed()
            {
                mapping.path = path.into();
            }
        });
    }
    if let Some(index) = remove_serial {
        options.serial_mappings.remove(index);
    }
    if ui
        .add_enabled(
            embedded && cfg!(any(unix, windows)) && options.serial_mappings.len() < 16,
            Button::new("Add serial mapping"),
        )
        .clicked()
    {
        let name = (1..=256)
            .map(|n| format!("COM{n}"))
            .find(|name| {
                !options
                    .serial_mappings
                    .iter()
                    .any(|mapping| mapping.name.eq_ignore_ascii_case(name))
            })
            .unwrap();
        options.serial_mappings.push(SerialMapping {
            name,
            path: Default::default(),
            read_only: true,
        });
    }
    {
        if (embedded && cfg!(any(target_os = "linux", target_os = "macos", windows)))
            || options.printer_mapping.is_some()
        {
            ui.label(RichText::new("Redirected printer").strong());
            if let Some(mapping) = &mut options.printer_mapping {
                ui.horizontal(|ui| {
                    ui.label("Local installed queue");
                    ui.text_edit_singleline(&mut mapping.queue);
                });
                ui.horizontal(|ui| {
                    ui.label("Windows driver name");
                    ui.text_edit_singleline(&mut mapping.driver);
                });
                ui.small("The remote Windows host needs the named driver. Select a local installed queue that accepts the driver's raw print data. The remote session can submit jobs to this queue.");
                if ui.button("Remove printer mapping").clicked() {
                    options.printer_mapping = None;
                }
            } else if ui.button("Add printer mapping").clicked() {
                options.printer_mapping = Some(PrinterMapping {
                    queue: String::new(),
                    driver: String::new(),
                });
            }
        }
        ui.small("Automatic printer, serial-port and smartcard redirection require the installed Windows client.");
        if !embedded || options.printers {
            ui.checkbox(&mut options.printers, "Redirect all printers");
        }
        ui.checkbox(&mut options.serial_ports, "Redirect serial ports");
        if !embedded || options.smartcard {
            ui.checkbox(&mut options.smartcard, "Redirect smartcards");
        }
        if let Some(reader) = &mut options.smartcard_reader {
            ui.horizontal(|ui| {
                ui.label("Local PC/SC reader name");
                ui.text_edit_singleline(reader);
            });
            ui.small("The remote session can use the card in this reader. Select it only for a server you trust.");
            if ui.button("Remove smartcard reader").clicked() {
                options.smartcard_reader = None;
            }
        } else if embedded
            && cfg!(any(target_os = "linux", target_os = "macos", windows))
            && ui.button("Add smartcard reader").clicked()
        {
            options.smartcard_reader = Some(String::new());
        }
    }
    let result = if embedded {
        options.negotiate(&RdpCapabilities::embedded()).map(|_| ())
    } else if cfg!(windows) {
        harbor_workspace::desktop::rdp_options::windows_profile(session).map(|_| ())
    } else {
        harbor_workspace::connection::freerdp_arguments(session).map(|_| ())
    };
    if let Err(error) = result {
        ui.colored_label(ui.visuals().error_fg_color, error);
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
    fn rdp_settings_controls_change_saved_clipboard_and_keyboard_choices() {
        use harbor_workspace::desktop::rdp_options::{ClipboardPolicy, KeyboardRouting};
        let ctx = Context::default();
        let mut s = Session {
            protocol: "RDP".into(),
            ..Default::default()
        };
        for (target, expected_clipboard, expected_keyboard) in [
            (
                "Both directions",
                ClipboardPolicy::Bidirectional,
                KeyboardRouting::Remote,
            ),
            (
                "Keep keyboard local",
                ClipboardPolicy::Bidirectional,
                KeyboardRouting::Local,
            ),
        ] {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 800.))),
                    ..Default::default()
                },
                |ui| session(ui, &mut s),
            );
            let point = output
                .shapes
                .iter()
                .find_map(|item| label(&item.shape, target))
                .expect("RDP control missing");
            output.textures_delta.clear();
            for pressed in [true, false] {
                let mut clicked = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 800.))),
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
            assert_eq!(s.rdp_options.clipboard, expected_clipboard);
            assert_eq!(s.rdp_options.keyboard, expected_keyboard);
        }
    }
    #[test]
    fn embedded_rdp_controls_add_read_only_folder_and_enable_admin() {
        let ctx = Context::default();
        let mut s = Session {
            protocol: "RDP".into(),
            ..Default::default()
        };
        for target in ["Administrative session", "Add folder mapping"] {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000., 1200.))),
                    ..Default::default()
                },
                |ui| session(ui, &mut s),
            );
            let point = output
                .shapes
                .iter()
                .find_map(|item| label(&item.shape, target))
                .expect("missing RDP control");
            output.textures_delta.clear();
            for pressed in [true, false] {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000., 1200.))),
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
                output.textures_delta.clear();
            }
        }
        assert!(s.rdp_options.administrative);
        assert_eq!(s.rdp_options.drives.len(), 1);
        assert!(s.rdp_options.drives[0].read_only);
        assert!(s.rdp_options.drives[0].path.as_os_str().is_empty());
        assert!(
            s.rdp_options.validate().is_err(),
            "an unselected folder must not be saved"
        );
    }
    #[test]
    fn switching_engines_can_clear_incompatible_saved_options() {
        let ctx = Context::default();
        let mut s = Session {
            protocol: "RDP".into(),
            ..Default::default()
        };
        s.rdp_options.smartcard = true;
        s.rdp_options
            .drives
            .push(harbor_workspace::desktop::rdp_options::DriveMapping {
                name: "Work".into(),
                path: std::env::temp_dir(),
                read_only: true,
            });
        for (engine, target) in [
            (
                harbor_workspace::model::DesktopEngine::Embedded,
                "Redirect smartcards",
            ),
            (harbor_workspace::model::DesktopEngine::External, "Remove"),
        ] {
            s.desktop_engine = engine;
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 1500.))),
                    ..Default::default()
                },
                |ui| session(ui, &mut s),
            );
            let point = output
                .shapes
                .iter()
                .find_map(|item| label(&item.shape, target));
            output.textures_delta.clear();
            let point = point.expect("incompatible saved option has no reachable clear control");
            for pressed in [true, false] {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 1500.))),
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
                output.textures_delta.clear();
            }
        }
        assert!(!s.rdp_options.smartcard);
        assert!(s.rdp_options.drives.is_empty());
    }
    #[test]
    fn explicit_serial_mapping_is_reachable_and_defaults_read_only() {
        let ctx = Context::default();
        let mut s = Session {
            protocol: "RDP".into(),
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 1800.))),
                ..Default::default()
            },
            |ui| session(ui, &mut s),
        );
        let point = output
            .shapes
            .iter()
            .find_map(|item| label(&item.shape, "Add serial mapping"));
        output.textures_delta.clear();
        let point = point.expect("No explicit serial mapping control");
        for pressed in [true, false] {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 1800.))),
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
            output.textures_delta.clear();
        }
        assert_eq!(s.rdp_options.serial_mappings.len(), 1);
        assert_eq!(s.rdp_options.serial_mappings[0].name, "COM1");
        assert!(s.rdp_options.serial_mappings[0].read_only);
        assert!(s.rdp_options.validate().is_err());
    }
}
