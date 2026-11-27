//! Explicit text transfers for the authenticated private XQuartz display.
use super::{XClientDisplay, native_transport, unix_connection};
use std::{
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{Event, xproto::*},
    wrapper::ConnectionExt as _,
};
use zeroize::Zeroizing;

const MAX_TEXT: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(2);

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn validate_text(text: &[u8]) -> Result<(), String> {
    if text.len() > MAX_TEXT || text.contains(&0) || std::str::from_utf8(text).is_err() {
        return Err("X11 clipboard requires UTF-8 text without NUL bytes, up to 64 KiB.".into());
    }
    Ok(())
}

pub struct MacosClipboard {
    connection: unix_connection::Conn,
    display: XClientDisplay,
    cancelled: Arc<AtomicBool>,
    owner: Window,
    clipboard: Atom,
    utf8: Atom,
    targets: Atom,
    timestamp: Atom,
    multiple: Atom,
    atom_pair: Atom,
    property: Atom,
    selection_time: Timestamp,
    text: Option<Zeroizing<String>>,
}

impl MacosClipboard {
    /// Only ready TermCraft display handles can open a clipboard connection.
    pub fn open(display: XClientDisplay, cancelled: Arc<AtomicBool>) -> Result<Self, String> {
        display.ensure_running()?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(display.authority_path())
            .map_err(error)?;
        if !file.metadata().map_err(error)?.is_file() {
            return Err("Private XQuartz clipboard authority must be a regular file.".into());
        }
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(1025).read_to_end(&mut bytes).map_err(error)?;
        if bytes.len() > 1024 {
            return Err("Private XQuartz clipboard authority exceeds the size limit.".into());
        }
        let cookie = crate::ssh::x11::authority_cookie(&bytes, 0, b"", true)?;
        let stream = native_transport::connect_unix(Path::new(display.display()), &cancelled)?;
        let connection = unix_connection::connect(
            stream,
            0,
            (b"MIT-MAGIC-COOKIE-1".to_vec(), cookie.to_vec()),
            cancelled.clone(),
            Instant::now() + READ_TIMEOUT,
        )
        .map_err(error)?;
        let owner = window(&connection)?;
        let atom = |name| {
            connection
                .intern_atom(false, name)
                .map_err(error)?
                .reply()
                .map(|reply| reply.atom)
                .map_err(error)
        };
        Ok(Self {
            owner,
            clipboard: atom(b"CLIPBOARD")?,
            utf8: atom(b"UTF8_STRING")?,
            targets: atom(b"TARGETS")?,
            timestamp: atom(b"TIMESTAMP")?,
            multiple: atom(b"MULTIPLE")?,
            atom_pair: atom(b"ATOM_PAIR")?,
            property: atom(b"TERMCRAFT_CLIPBOARD")?,
            selection_time: x11rb::CURRENT_TIME,
            connection,
            display,
            cancelled,
            text: None,
        })
    }

    /// Replace the X11 selection only on an explicit macOS-to-X11 transfer.
    pub fn write_text(&mut self, text: String) -> Result<(), String> {
        let text = Zeroizing::new(text);
        validate_text(text.as_bytes())?;
        self.poll()?;
        // Separate owner windows bind delayed requests to their publication.
        let owner = window(&self.connection)?;
        let result = self.publish(owner);
        if result.is_err() {
            let _ = self.connection.destroy_window(owner);
            let _ = self.connection.flush();
        }
        self.selection_time = result?;
        let previous = std::mem::replace(&mut self.owner, owner);
        self.text = Some(text);
        let _ = self.connection.destroy_window(previous);
        self.connection.flush().map_err(error)
    }

    fn publish(&mut self, owner: Window) -> Result<Timestamp, String> {
        self.connection
            .change_property8(
                PropMode::REPLACE,
                owner,
                self.property,
                AtomEnum::INTEGER,
                &[0],
            )
            .map_err(error)?
            .check()
            .map_err(error)?;
        let deadline = Instant::now() + READ_TIMEOUT;
        while Instant::now() < deadline && !self.cancelled.load(Ordering::Relaxed) {
            unix_connection::renew(&self.connection);
            let Some(event) = self.connection.poll_for_event().map_err(error)? else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            if let Event::PropertyNotify(notice) = event
                && notice.window == owner
                && notice.atom == self.property
                && notice.state == Property::NEW_VALUE
                && notice.response_type & 0x80 == 0
            {
                self.connection
                    .set_selection_owner(owner, self.clipboard, notice.time)
                    .map_err(error)?
                    .check()
                    .map_err(error)?;
                return if self.selection_owner()? == owner {
                    Ok(notice.time)
                } else {
                    Err("Could not take ownership of the private X11 clipboard.".into())
                };
            }
            self.selection_event(event)?;
        }
        Err("The private X11 clipboard ownership request expired or was cancelled.".into())
    }

    pub fn read_text(&mut self) -> Result<String, String> {
        self.poll()?;
        match self.selection_owner()? {
            x11rb::NONE => return Err("The private X11 clipboard is empty.".into()),
            owner if owner == self.owner => {
                return self
                    .text
                    .as_deref()
                    .cloned()
                    .ok_or_else(|| "The private X11 clipboard is empty.".into());
            }
            _ => {}
        }
        // Give each read its own window so a late reply can never complete
        // the next request.
        let requestor = window(&self.connection)?;
        let result = self.read_selection(requestor);
        let _ = self.connection.destroy_window(requestor);
        let _ = self.connection.flush();
        result
    }

    pub fn poll(&mut self) -> Result<(), String> {
        self.display.ensure_running()?;
        unix_connection::renew(&self.connection);
        for _ in 0..32 {
            let Some(event) = self.connection.poll_for_event().map_err(error)? else {
                break;
            };
            self.selection_event(event)?;
        }
        Ok(())
    }

    fn selection_owner(&self) -> Result<Window, String> {
        self.connection
            .get_selection_owner(self.clipboard)
            .map_err(error)?
            .reply()
            .map(|reply| reply.owner)
            .map_err(error)
    }

    fn read_selection(&mut self, requestor: Window) -> Result<String, String> {
        let deadline = Instant::now() + READ_TIMEOUT;
        self.connection
            .convert_selection(
                requestor,
                self.clipboard,
                self.utf8,
                self.property,
                x11rb::CURRENT_TIME,
            )
            .map_err(error)?
            .check()
            .map_err(error)?;
        while Instant::now() < deadline && !self.cancelled.load(Ordering::Relaxed) {
            self.display.ensure_running()?;
            unix_connection::renew(&self.connection);
            let Some(event) = self.connection.poll_for_event().map_err(error)? else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            if let Event::SelectionNotify(notice) = event
                && notice.requestor == requestor
                && notice.selection == self.clipboard
                && notice.target == self.utf8
            {
                if notice.property != self.property {
                    return Err("The X11 clipboard owner did not supply UTF-8 text.".into());
                }
                let reply = self
                    .connection
                    .get_property(
                        true,
                        requestor,
                        self.property,
                        self.utf8,
                        0,
                        MAX_TEXT as u32 / 4,
                    )
                    .map_err(error)?
                    .reply()
                    .map_err(error)?;
                if reply.type_ != self.utf8 || reply.format != 8 || reply.bytes_after != 0 {
                    return Err(
                        "X11 clipboard requires a single UTF-8 transfer up to 64 KiB.".into(),
                    );
                }
                validate_text(&reply.value)?;
                return String::from_utf8(reply.value).map_err(error);
            }
            self.selection_event(event)?;
        }
        Err("The private X11 clipboard request expired or was cancelled.".into())
    }

    fn selection_event(&mut self, event: Event) -> Result<(), String> {
        match event {
            Event::SelectionClear(clear)
                if clear.owner == self.owner && clear.selection == self.clipboard =>
            {
                if self.selection_owner()? != self.owner {
                    self.text = None;
                }
            }
            Event::SelectionRequest(request) => {
                let older = request.time != x11rb::CURRENT_TIME
                    && (request.time.wrapping_sub(self.selection_time) as i32) < 0;
                if request.owner != self.owner || request.selection != self.clipboard || older {
                    return self.notify(request, x11rb::NONE);
                }
                let property = if request.property == x11rb::NONE {
                    request.target
                } else {
                    request.property
                };
                let accepted = if request.target == self.multiple {
                    request.property != x11rb::NONE
                        && self.write_multiple(request.requestor, property)?
                } else {
                    self.write_target(request.requestor, property, request.target)?
                };
                self.notify(request, if accepted { property } else { x11rb::NONE })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn write_target(
        &self,
        requestor: Window,
        property: Atom,
        target: Atom,
    ) -> Result<bool, String> {
        let Some(text) = &self.text else {
            return Ok(false);
        };
        let conn = &self.connection;
        let sent = if target == self.utf8 {
            conn.change_property8(
                PropMode::REPLACE,
                requestor,
                property,
                self.utf8,
                text.as_bytes(),
            )
        } else if target == self.targets {
            conn.change_property32(
                PropMode::REPLACE,
                requestor,
                property,
                AtomEnum::ATOM,
                &[self.targets, self.timestamp, self.multiple, self.utf8],
            )
        } else if target == self.timestamp {
            conn.change_property32(
                PropMode::REPLACE,
                requestor,
                property,
                AtomEnum::INTEGER,
                &[self.selection_time],
            )
        } else {
            return Ok(false);
        };
        Ok(sent.map_err(error)?.check().is_ok())
    }

    fn write_multiple(&self, requestor: Window, property: Atom) -> Result<bool, String> {
        let reply = self
            .connection
            .get_property(false, requestor, property, self.atom_pair, 0, 128)
            .map_err(error)?
            .reply();
        let Ok(reply) = reply else {
            return Ok(false);
        };
        if reply.type_ != self.atom_pair || reply.bytes_after != 0 || reply.value_len % 2 != 0 {
            return Ok(false);
        }
        let Some(values) = reply.value32() else {
            return Ok(false);
        };
        let mut pairs: Vec<_> = values.collect();
        for pair in pairs.chunks_exact_mut(2) {
            let target_property = pair[1];
            if target_property == x11rb::NONE
                || target_property == property
                || !self.write_target(requestor, target_property, pair[0])?
            {
                pair[1] = x11rb::NONE;
            }
        }
        Ok(self
            .connection
            .change_property32(
                PropMode::REPLACE,
                requestor,
                property,
                self.atom_pair,
                &pairs,
            )
            .map_err(error)?
            .check()
            .is_ok())
    }

    fn notify(&self, request: SelectionRequestEvent, property: Atom) -> Result<(), String> {
        self.connection
            .send_event(
                false,
                request.requestor,
                EventMask::NO_EVENT,
                SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: request.time,
                    requestor: request.requestor,
                    selection: request.selection,
                    target: request.target,
                    property,
                },
            )
            .map_err(error)?;
        self.connection.flush().map_err(error)
    }
}

fn window(connection: &unix_connection::Conn) -> Result<Window, String> {
    let id = connection.generate_id().map_err(error)?;
    let root = connection.setup().roots[0].root;
    connection
        .create_window(
            0,
            id,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(error)?
        .check()
        .map_err(error)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x11::{XServerManager, XServerOptions};

    #[test]
    fn private_clipboard_rejects_invalid_and_oversized_content() {
        assert!(validate_text("selection 🪟".as_bytes()).is_ok());
        assert!(validate_text(&vec![b'a'; MAX_TEXT]).is_ok());
        assert!(validate_text(&vec![b'a'; MAX_TEXT + 1]).is_err());
        assert!(validate_text(b"embedded\0nul").is_err());
        assert!(validate_text(&[0xff]).is_err());
    }

    #[test]
    #[ignore = "requires the staged private XQuartz runtime and a macOS desktop"]
    fn private_clipboard_round_trips_utf8_and_releases_its_selection() {
        let root = std::env::var_os("TERMCRAFT_TEST_XQUARTZ_ROOT").expect("staged Resources");
        let lease = XServerManager::for_installation(root.into())
            .start(XServerOptions::default())
            .unwrap();
        let display = lease.client_display();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut first = MacosClipboard::open(display.clone(), cancelled.clone()).unwrap();
        let mut second = MacosClipboard::open(display, cancelled).unwrap();
        assert!(first.read_text().unwrap_err().contains("empty"));
        transfer(&mut first, &mut second, "mac → X11 café\n✓");
        let previous = first.owner;
        first.write_text("replacement".into()).unwrap();
        assert_ne!(previous, first.owner);
        unix_connection::renew(&second.connection);
        first
            .selection_event(Event::SelectionRequest(SelectionRequestEvent {
                response_type: SELECTION_REQUEST_EVENT,
                sequence: 0,
                time: x11rb::CURRENT_TIME,
                owner: previous,
                requestor: second.owner,
                selection: first.clipboard,
                target: first.utf8,
                property: second.property,
            }))
            .unwrap();
        assert_eq!(notification(&mut first, &second).property, x11rb::NONE);
        check_multiple(&mut first, &second, "replacement");
        transfer(&mut second, &mut first, "X11 → mac 漢字\n🦀");
        assert!(
            first.text.is_none(),
            "lost selection should discard old text"
        );
        assert!(second.write_text("invalid\0text".into()).is_err());
        assert_eq!(second.read_text().unwrap(), "X11 → mac 漢字\n🦀");
        drop(second);
        assert!(first.read_text().unwrap_err().contains("empty"));
        lease.stop_owned().unwrap();
        assert!(first.read_text().is_err());
    }

    fn transfer(sender: &mut MacosClipboard, receiver: &mut MacosClipboard, text: &str) {
        sender.write_text(text.into()).unwrap();
        std::thread::scope(|scope| {
            let read = scope.spawn(|| receiver.read_text());
            while !read.is_finished() {
                sender.poll().unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(read.join().unwrap().unwrap(), text);
        });
    }

    fn notification(owner: &mut MacosClipboard, peer: &MacosClipboard) -> SelectionNotifyEvent {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            owner.poll().unwrap();
            unix_connection::renew(&peer.connection);
            if let Some(Event::SelectionNotify(notice)) = peer.connection.poll_for_event().unwrap()
            {
                return notice;
            }
            assert!(
                Instant::now() < deadline,
                "X11 did not deliver the selection result"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn check_multiple(owner: &mut MacosClipboard, peer: &MacosClipboard, text: &str) {
        unix_connection::renew(&peer.connection);
        let pairs = [
            peer.timestamp,
            peer.timestamp,
            peer.utf8,
            peer.utf8,
            AtomEnum::PRIMARY.into(),
            peer.targets,
        ];
        peer.connection
            .change_property32(
                PropMode::REPLACE,
                peer.owner,
                peer.property,
                peer.atom_pair,
                &pairs,
            )
            .unwrap()
            .check()
            .unwrap();
        peer.connection
            .convert_selection(
                peer.owner,
                peer.clipboard,
                peer.multiple,
                peer.property,
                x11rb::CURRENT_TIME,
            )
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(notification(owner, peer).property, peer.property);
        let property = |name, kind, units| {
            peer.connection
                .get_property(false, peer.owner, name, kind, 0, units)
                .unwrap()
                .reply()
                .unwrap()
        };
        let mut converted = pairs;
        converted[5] = x11rb::NONE;
        assert_eq!(
            property(peer.property, peer.atom_pair, 6)
                .value32()
                .unwrap()
                .collect::<Vec<_>>(),
            converted
        );
        assert_eq!(
            property(peer.timestamp, AtomEnum::INTEGER.into(), 1)
                .value32()
                .unwrap()
                .next(),
            Some(owner.selection_time)
        );
        assert_eq!(property(peer.utf8, peer.utf8, 64).value, text.as_bytes());
    }
}
