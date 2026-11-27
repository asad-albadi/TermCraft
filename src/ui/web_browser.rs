use super::*;
use harbor_workspace::browser::{Command, Credentials, Session as BrowserSession, bounded_size};
use zeroize::Zeroizing;

pub(super) struct State {
    pub handle: BrowserSession,
    address: String,
    address_dirty: bool,
    texture: Option<TextureHandle>,
    viewport: [u32; 2],
    viewport_revision: u64,
    texture_revision: Option<u64>,
    painted: Option<PaintedViewport>,
    frame_size: [u32; 2],
    auth: Option<Auth>,
    input_error: Option<&'static str>,
    pointer: Option<PointerCapture>,
}

struct PaintedViewport {
    revision: u64,
    viewport: ViewportId,
    frame: u64,
}

#[derive(Clone, Copy)]
struct PointerCapture {
    button: &'static str,
    pending_release: Option<PointerRelease>,
}

#[derive(Clone, Copy)]
enum PointerRelease {
    At(f64, f64),
    Cancel,
}

struct Auth {
    challenge: u64,
    username: Zeroizing<String>,
    password: Zeroizing<String>,
}

impl State {
    pub fn new(handle: BrowserSession) -> Self {
        let address = handle.state().address;
        Self {
            handle,
            address,
            address_dirty: false,
            texture: None,
            viewport: [960, 640],
            viewport_revision: 0,
            texture_revision: None,
            painted: None,
            frame_size: [960, 640],
            auth: None,
            input_error: None,
            pointer: None,
        }
    }

    fn send(&mut self, command: Command) -> bool {
        self.input_error = self.handle.send(command).err();
        self.input_error.is_none()
    }

    fn release_pointer(&mut self, release: PointerRelease) {
        if let Some(pointer) = &mut self.pointer
            && (pointer.pending_release.is_none() || matches!(release, PointerRelease::Cancel))
        {
            pointer.pending_release = Some(release);
        }
        self.flush_pointer_release();
    }

    fn flush_pointer_release(&mut self) -> bool {
        let Some(PointerCapture {
            button,
            pending_release: Some(release),
        }) = self.pointer
        else {
            return true;
        };
        let command = match release {
            PointerRelease::At(x, y) => Command::Pointer {
                kind: "mouseReleased",
                button,
                x,
                y,
            },
            PointerRelease::Cancel => Command::CancelPointer,
        };
        if !self.send(command) {
            return false;
        }
        self.pointer = None;
        true
    }
}

pub(super) fn settings(ui: &mut Ui, session: &mut Session) {
    let options = &mut session.browser_options;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut options.embedded, true, "Embedded in TermCraft");
        if ui
            .selectable_value(&mut options.embedded, false, "Default system browser")
            .clicked()
        {
            options.proxy.clear();
        }
    });
    if options.embedded {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut options.runtime,
                harbor_workspace::browser::Runtime::BundledChromium,
                "Packaged Chromium",
            );
            ui.selectable_value(
                &mut options.runtime,
                harbor_workspace::browser::Runtime::SystemChromium,
                "Installed Chromium (compatibility)",
            );
        });
        ui.horizontal(|ui| {
            ui.label("Proxy endpoint");
            ui.add(TextEdit::singleline(&mut options.proxy).hint_text("http://proxy.example:8080"));
        });
        #[cfg(windows)]
        ui.small("Requires a native 64-bit installation of Chrome or Chromium.");
        if !cfg!(any(target_os = "linux", target_os = "macos", windows)) {
            ui.small("Embedded Browser is unavailable on this platform.");
        }
    }
}

pub(super) fn view(ui: &mut Ui, tab: &mut TerminalTab) {
    let Some(browser) = &mut tab.web_browser else {
        ui.colored_label(ui.visuals().error_fg_color, &tab.error);
        return;
    };
    let state = browser.handle.state();
    if state.challenge.is_some() || !ui.is_enabled() || !ui.input(|input| input.focused) {
        browser.release_pointer(PointerRelease::Cancel);
    }
    if !state.closed {
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
    if let Some(frame) = browser.handle.take_frame().filter(|frame| {
        frame.generation == tab.id
            && frame.viewport_revision == browser.viewport_revision
            && frame.size == browser.viewport
    }) {
        browser.frame_size = frame.size;
        browser.texture_revision = Some(frame.viewport_revision);
        let image = ColorImage::from_rgba_unmultiplied(frame.size.map(|v| v as usize), &frame.rgba);
        if let Some(texture) = browser.texture.as_mut() {
            texture.set(image, TextureOptions::LINEAR);
        } else {
            browser.texture = Some(ui.ctx().load_texture(
                format!("website-{}", tab.id),
                image,
                TextureOptions::LINEAR,
            ));
        }
    }
    if let Some(text) = browser.handle.take_copy() {
        ui.ctx().copy_text(text);
    }
    let address_id = ui.make_persistent_id(("website-address", tab.id));
    if !browser.address_dirty {
        browser.address.clone_from(&state.address);
    }
    ui.add_enabled_ui(!state.closed, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(state.can_back, Button::new("Back"))
                .clicked()
            {
                browser.send(Command::History(-1));
            }
            if ui
                .add_enabled(state.can_forward, Button::new("Forward"))
                .clicked()
            {
                browser.send(Command::History(1));
            }
            if state.loading {
                if ui.button("Stop").clicked() {
                    browser.send(Command::Stop);
                }
            } else if ui.button("Reload").clicked() {
                browser.send(Command::Reload);
            }
            let response = ui.add(
                TextEdit::singleline(&mut browser.address)
                    .id(address_id)
                    .char_limit(8192)
                    .desired_width((ui.available_width() - 45.).max(90.)),
            );
            browser.address_dirty |= response.changed();
            let go = ui.button("Go");
            let submit = go.clicked()
                || (response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)));
            if submit {
                browser.send(Command::Navigate(browser.address.clone()));
            }
            if submit || (!response.has_focus() && !go.is_pointer_button_down_on()) {
                browser.address_dirty = false;
                browser.address.clone_from(&state.address);
            }
        });
    });
    if let Some(error) = browser.input_error {
        ui.add(Label::new(error).truncate());
    } else if state.loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading page…");
        });
    } else if !state.title.is_empty() {
        ui.add(Label::new(&state.title).truncate());
    }
    if let Some(error) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if let Some(notice) = &state.notice {
        ui.small(notice);
    }

    if let Some(challenge) = &state.challenge {
        if browser
            .auth
            .as_ref()
            .is_none_or(|auth| auth.challenge != challenge.id)
        {
            browser.auth = Some(Auth {
                challenge: challenge.id,
                username: Zeroizing::default(),
                password: Zeroizing::default(),
            });
        }
        let auth = browser.auth.as_mut().unwrap();
        let command = ui
            .group(|ui| {
                ui.label(if challenge.proxy {
                    "Proxy authentication"
                } else {
                    "Website authentication"
                });
                ui.label(&challenge.origin);
                for (value, hint, password) in [
                    (&mut auth.username, "Username", false),
                    (&mut auth.password, "Password", true),
                ] {
                    let mut output = TextEdit::singleline(&mut **value)
                        .id_salt((tab.id, challenge.id, hint))
                        .hint_text(hint)
                        .password(password)
                        .char_limit(if password { 4096 } else { 1024 })
                        .show(ui);
                    // Keep the challenge fields out of egui's cross-frame undo history.
                    output.state.clear_undoer();
                    output.state.store(ui.ctx(), output.response.id);
                }
                ui.horizontal(|ui| {
                    let send = ui.button("Sign in").clicked();
                    let cancel = ui.button("Cancel").clicked();
                    (send || cancel).then(|| Command::Auth {
                        challenge: auth.challenge,
                        credentials: send.then(|| Credentials {
                            username: auth.username.clone(),
                            password: auth.password.clone(),
                        }),
                    })
                })
                .inner
            })
            .inner;
        if command.is_some_and(|command| browser.send(command)) {
            browser.auth = None;
        }
    } else {
        browser.auth = None;
    }

    let available = ui.available_size().max(Vec2::splat(1.));
    let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
    let requested = bounded_size([rect.width() as u32, rect.height() as u32]);
    if !state.closed && requested != browser.viewport {
        browser.release_pointer(PointerRelease::Cancel);
        let revision = browser.viewport_revision + 1;
        if browser.pointer.is_none()
            && browser.send(Command::Resize {
                size: requested,
                revision,
            })
        {
            browser.viewport = requested;
            browser.viewport_revision = revision;
            browser.painted = None;
        }
    } else {
        browser.flush_pointer_release();
    }
    let viewport = ui.ctx().viewport_id();
    let frame = ui.ctx().cumulative_frame_nr();
    let pointer_ready = ui.input(|input| input.focused)
        && requested == browser.viewport
        && browser.painted.as_ref().is_some_and(|painted| {
            painted.revision == browser.viewport_revision
                && painted.viewport == viewport
                && painted.frame < frame
        });
    let Some(texture) = &browser.texture else {
        return;
    };
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
        Color32::WHITE,
    );
    // Input collected for this UI frame was generated before this paint.
    if browser.texture_revision == Some(browser.viewport_revision)
        && browser.painted.as_ref().is_none_or(|painted| {
            painted.viewport != viewport || painted.revision != browser.viewport_revision
        })
    {
        browser.painted = Some(PaintedViewport {
            revision: browser.viewport_revision,
            viewport,
            frame,
        });
    }
    if state.challenge.is_some() || state.closed || !ui.is_enabled() {
        return;
    }
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.has_focus() {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            )
        });
        ui.output_mut(|output| {
            output.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect,
                cursor_rect: Rect::from_min_size(rect.left_top(), vec2(1., 18.)),
                should_interrupt_composition: false,
            })
        });
    }
    let events = ui.input(|input| input.events.clone());
    for event in events {
        forward(ui, browser, event, &response, rect, pointer_ready);
    }
}

fn forward(
    ui: &mut Ui,
    browser: &mut State,
    event: Event,
    response: &Response,
    rect: Rect,
    pointer_ready: bool,
) {
    if matches!(event, Event::PointerGone | Event::WindowFocused(false)) {
        browser.release_pointer(PointerRelease::Cancel);
        return;
    }
    if !browser.flush_pointer_release() {
        return;
    }
    let point = |pos: Pos2, size: [u32; 2]| {
        let delta = pos - rect.min;
        (
            f64::from(delta.x / rect.width()) * f64::from(size[0]),
            f64::from(delta.y / rect.height()) * f64::from(size[1]),
        )
    };
    let owns_point = |pos| rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id());
    let command = match event {
        Event::PointerMoved(pos)
            if pointer_ready && (owns_point(pos) || browser.pointer.is_some()) =>
        {
            let (x, y) = point(pos, browser.frame_size);
            Some(Command::Pointer {
                kind: "mouseMoved",
                button: browser.pointer.map_or("none", |pointer| pointer.button),
                x,
                y,
            })
        }
        Event::PointerButton {
            pos,
            button,
            pressed,
            ..
        } => {
            let button = match button {
                PointerButton::Primary => "left",
                PointerButton::Secondary => "right",
                PointerButton::Middle => "middle",
                _ => return,
            };
            let (x, y) = point(pos, browser.frame_size);
            if pressed && pointer_ready && browser.pointer.is_none() && owns_point(pos) {
                response.request_focus();
                if browser.send(Command::Pointer {
                    kind: "mousePressed",
                    button,
                    x,
                    y,
                }) {
                    browser.pointer = Some(PointerCapture {
                        button,
                        pending_release: None,
                    });
                }
            } else if !pressed
                && browser
                    .pointer
                    .is_some_and(|pointer| pointer.button == button)
            {
                let release = if pointer_ready {
                    PointerRelease::At(x, y)
                } else {
                    PointerRelease::Cancel
                };
                browser.release_pointer(release);
            }
            None
        }
        Event::MouseWheel { unit, delta, .. } if pointer_ready && response.hovered() => {
            let Some(pos) = ui.input(|input| input.pointer.hover_pos()) else {
                return;
            };
            let (x, y) = point(pos, browser.frame_size);
            let multiplier = match unit {
                MouseWheelUnit::Point => 1.,
                MouseWheelUnit::Line => 24.,
                MouseWheelUnit::Page => rect.height(),
            };
            Some(Command::Wheel {
                x,
                y,
                dx: f64::from(-delta.x * multiplier),
                dy: f64::from(-delta.y * multiplier),
            })
        }
        Event::Text(text) | Event::Paste(text) | Event::Ime(egui::ImeEvent::Commit(text))
            if response.has_focus() =>
        {
            Some(Command::Text(text))
        }
        Event::Ime(egui::ImeEvent::Preedit {
            text,
            active_range_chars,
        }) if response.has_focus() => {
            let offset = |chars| text.chars().take(chars).map(char::len_utf16).sum();
            let selection = active_range_chars
                .map(|range| [offset(range.start), offset(range.end)])
                .unwrap_or_else(|| [text.encode_utf16().count(); 2]);
            Some(Command::Composition { text, selection })
        }
        Event::Copy if response.has_focus() => Some(Command::CopySelection),
        Event::Key {
            key,
            pressed,
            modifiers,
            repeat,
            ..
        } if response.has_focus() => browser_key(key, pressed, modifiers, repeat),
        _ => None,
    };
    if let Some(command) = command {
        browser.send(command);
    }
}

fn browser_key(key: Key, down: bool, modifiers: Modifiers, repeat: bool) -> Option<Command> {
    if modifiers.command && matches!(key, Key::C | Key::V | Key::X) {
        return None;
    }
    let (name, virtual_key) = match key {
        Key::Backspace => ("Backspace", 8),
        Key::Tab => ("Tab", 9),
        Key::Enter => ("Enter", 13),
        Key::Escape => ("Escape", 27),
        Key::Space => (" ", 32),
        Key::PageUp => ("PageUp", 33),
        Key::PageDown => ("PageDown", 34),
        Key::End => ("End", 35),
        Key::Home => ("Home", 36),
        Key::ArrowLeft => ("ArrowLeft", 37),
        Key::ArrowUp => ("ArrowUp", 38),
        Key::ArrowRight => ("ArrowRight", 39),
        Key::ArrowDown => ("ArrowDown", 40),
        Key::Insert => ("Insert", 45),
        Key::Delete => ("Delete", 46),
        Key::Minus => ("-", 189),
        Key::Quote => ("'", 222),
        Key::Copy | Key::Cut | Key::Paste => return None,
        other => (other.symbol_or_name(), 0),
    };
    let (key, code, virtual_key) = match name.as_bytes() {
        [letter @ b'A'..=b'Z'] => (
            if modifiers.shift {
                name.to_owned()
            } else {
                name.to_ascii_lowercase()
            },
            format!("Key{name}"),
            u32::from(*letter),
        ),
        [digit @ b'0'..=b'9'] => (name.into(), format!("Digit{name}"), u32::from(*digit)),
        _ => (name.into(), key_code(name, key), virtual_key),
    };
    let modifiers = u8::from(modifiers.alt)
        | u8::from(modifiers.ctrl) << 1
        | u8::from(modifiers.mac_cmd) << 2
        | u8::from(modifiers.shift) << 3;
    Some(Command::Key {
        down,
        key,
        code,
        virtual_key,
        modifiers,
        repeat,
    })
}

fn key_code(name: &str, key: Key) -> String {
    if name.len() == 1 {
        key.name().into()
    } else {
        name.into()
    }
}

#[cfg(all(
    test,
    any(
        target_os = "linux",
        all(target_os = "macos", feature = "macos-chromium-qualification")
    )
))]
#[path = "web_browser_tests.rs"]
mod tests;
