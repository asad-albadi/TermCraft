//! CLIPBOARD selection only; never PRIMARY or obsolete CUT_BUFFER properties.
use super::*;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;
use zeroize::Zeroizing;
pub(super) struct Clipboard {
    window: u32,
    selection: u32,
    utf8: u32,
    targets: u32,
    property: u32,
    owner: u32,
    pending: Option<Instant>,
    text: Option<Zeroizing<String>>,
    next_read: Instant,
    fallback: bool,
    published: bool,
}
impl Clipboard {
    pub(super) fn open(conn: &RustConnection<Timed>, root: u32) -> io::Result<Self> {
        let atom = |name: &[u8]| -> io::Result<u32> {
            Ok(conn
                .intern_atom(false, name)
                .map_err(error)?
                .reply()
                .map_err(error)?
                .atom)
        };
        let selection = atom(b"CLIPBOARD")?;
        let utf8 = atom(b"UTF8_STRING")?;
        let targets = atom(b"TARGETS")?;
        let property = atom(b"_TERMCRAFT_VNC_CLIPBOARD")?;
        let window = conn.generate_id().map_err(error)?;
        conn.create_window(
            0,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .map_err(error)?
        .check()
        .map_err(error)?;
        Ok(Self {
            window,
            selection,
            utf8,
            targets,
            property,
            owner: 0,
            pending: None,
            text: None,
            next_read: Instant::now(),
            fallback: false,
            published: false,
        })
    }
    pub(super) fn set(
        &mut self,
        conn: &RustConnection<Timed>,
        text: Zeroizing<String>,
    ) -> io::Result<()> {
        if text.len() > 8192 || text.contains('\0') {
            return Err(error("Clipboard text exceeds limit"));
        }
        conn.set_selection_owner(self.window, self.selection, x11rb::CURRENT_TIME)
            .map_err(error)?
            .check()
            .map_err(error)?;
        if conn
            .get_selection_owner(self.selection)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .owner
            != self.window
        {
            return Err(error("Could not own selected clipboard"));
        }
        self.text = Some(text);
        self.published = false;
        self.owner = self.window;
        self.pending = None;
        Ok(())
    }
    pub(super) fn poll(
        &mut self,
        conn: &RustConnection<Timed>,
    ) -> io::Result<Option<Zeroizing<String>>> {
        let mut changed = None;
        // Bound event work per capture tick, including hostile requestors.
        for _ in 0..64 {
            let Some(event) = conn.poll_for_event().map_err(error)? else {
                break;
            };
            match event {
                Event::SelectionRequest(e)
                    if e.selection == self.selection && e.owner == self.window =>
                {
                    let p = if e.property == 0 {
                        e.target
                    } else {
                        e.property
                    };
                    let mut accepted = 0;
                    if let Some(text) = self.text.as_ref() {
                        let result = if e.target == self.targets {
                            conn.change_property32(
                                PropMode::REPLACE,
                                e.requestor,
                                p,
                                AtomEnum::ATOM,
                                &[self.targets, self.utf8, AtomEnum::STRING.into()],
                            )
                        } else if e.target == self.utf8 {
                            conn.change_property8(
                                PropMode::REPLACE,
                                e.requestor,
                                p,
                                self.utf8,
                                text.as_bytes(),
                            )
                        } else if e.target == u32::from(AtomEnum::STRING)
                            && text.chars().all(|c| c as u32 <= 255)
                        {
                            let bytes =
                                Zeroizing::new(text.chars().map(|c| c as u8).collect::<Vec<_>>());
                            conn.change_property8(
                                PropMode::REPLACE,
                                e.requestor,
                                p,
                                AtomEnum::STRING,
                                &bytes,
                            )
                        } else {
                            let reply = SelectionNotifyEvent {
                                response_type: SELECTION_NOTIFY_EVENT,
                                sequence: 0,
                                time: e.time,
                                requestor: e.requestor,
                                selection: e.selection,
                                target: e.target,
                                property: 0,
                            };
                            let _ = conn
                                .send_event(false, e.requestor, EventMask::NO_EVENT, reply)
                                .map_err(error)?
                                .check();
                            continue;
                        };
                        if result.map_err(error)?.check().is_ok() {
                            accepted = p;
                        }
                    }
                    let reply = SelectionNotifyEvent {
                        response_type: SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: e.time,
                        requestor: e.requestor,
                        selection: e.selection,
                        target: e.target,
                        property: accepted,
                    };
                    let _ = conn
                        .send_event(false, e.requestor, EventMask::NO_EVENT, reply)
                        .map_err(error)?
                        .check();
                }
                Event::SelectionClear(e) if e.selection == self.selection => {
                    self.text = None;
                }
                Event::SelectionNotify(e)
                    if e.requestor == self.window
                        && e.selection == self.selection
                        && self.pending.is_some() =>
                {
                    self.pending = None;
                    if e.property != self.property {
                        if e.property == 0 && !self.fallback {
                            self.fallback = true;
                            conn.convert_selection(
                                self.window,
                                self.selection,
                                AtomEnum::STRING.into(),
                                self.property,
                                x11rb::CURRENT_TIME,
                            )
                            .map_err(error)?
                            .check()
                            .map_err(error)?;
                            self.pending = Some(Instant::now());
                        }
                        continue;
                    }
                    let p = conn
                        .get_property(true, self.window, self.property, AtomEnum::ANY, 0, 2049)
                        .map_err(error)?
                        .reply()
                        .map_err(error)?;
                    // INCR and other target types are explicitly unsupported, never interpreted as text.
                    if p.bytes_after != 0
                        || p.format != 8
                        || p.type_
                            != if self.fallback {
                                u32::from(AtomEnum::STRING)
                            } else {
                                self.utf8
                            }
                        || p.value.len() > 8192
                    {
                        if self.published {
                            changed = Some(Zeroizing::new(String::new()));
                            self.published = false;
                            self.text = None;
                        }
                        continue;
                    }
                    let bytes = Zeroizing::new(p.value);
                    let text = if self.fallback {
                        Some(Zeroizing::new(
                            bytes.iter().map(|b| char::from(*b)).collect::<String>(),
                        ))
                    } else {
                        std::str::from_utf8(&bytes)
                            .ok()
                            .map(|text| Zeroizing::new(text.to_owned()))
                    };
                    if let Some(t) = text {
                        if t.len() > 8192 {
                            continue;
                        }
                        if !t.contains('\0')
                            && self.text.as_deref().map(String::as_str) != Some(t.as_str())
                        {
                            self.text = Some(t.clone());
                            changed = Some(t);
                            self.published = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if self
            .pending
            .is_some_and(|t| t.elapsed() > Duration::from_secs(1))
        {
            self.pending = None;
            conn.delete_property(self.window, self.property)
                .map_err(error)?;
        }
        let owner = conn
            .get_selection_owner(self.selection)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .owner;
        if owner != self.owner {
            if self.published {
                changed = Some(Zeroizing::new(String::new()));
                self.published = false;
            }
            self.owner = owner;
            self.pending = None;
            self.text = None;
            self.next_read = Instant::now();
        }
        if owner != 0
            && owner != self.window
            && self.pending.is_none()
            && Instant::now() >= self.next_read
        {
            self.fallback = false;
            conn.convert_selection(
                self.window,
                self.selection,
                self.utf8,
                self.property,
                x11rb::CURRENT_TIME,
            )
            .map_err(error)?
            .check()
            .map_err(error)?;
            self.pending = Some(Instant::now());
            self.next_read = Instant::now() + Duration::from_millis(500);
        }
        conn.flush().map_err(error)?;
        Ok(changed)
    }
}
