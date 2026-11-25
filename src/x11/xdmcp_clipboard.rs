//! Lease-owned, opt-in UTF-8 CLIPBOARD bridge between two local X11 servers.
//! PRIMARY, raw binary formats, and the Wayland compositor clipboard are out of scope.
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xfixes::{ConnectionExt as _, SelectionEventMask},
        xproto::*,
    },
    wrapper::ConnectionExt as _,
};
use zeroize::{Zeroize, Zeroizing};
mod transport;
use transport::Conn;
pub(super) fn validate_display(display: &str) -> Result<(u16, usize), String> {
    transport::local_display(display).map_err(|e| e.to_string())
}
const MAX_TEXT: usize = 1024 * 1024;
const CHUNK: usize = 16 * 1024;
const TRANSFER_TIME: Duration = Duration::from_secs(2);
fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}
fn validate_text(bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_TEXT || bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return Err(error(
            "XDMCP clipboard accepts UTF-8 text without NUL, at most 1 MiB",
        ));
    }
    Ok(())
}
fn validate_incr_complete(bytes: &[u8], minimum: usize) -> io::Result<()> {
    if bytes.len() < minimum {
        return Err(error("Incomplete INCR clipboard transfer"));
    }
    validate_text(bytes)
}
pub(super) struct Bridge {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Bridge {
    pub(super) fn start(
        display: &str,
        authority: Option<&std::path::Path>,
        nested: std::os::unix::net::UnixStream,
        cookie: &[u8],
        nested_pid: u32,
        socket: &std::path::Path,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        if transport::peer(&nested).map_err(|e| e.to_string())?.pid != nested_pid as i32 {
            return Err("Nested clipboard peer identity changed".into());
        }
        use std::os::fd::FromRawFd;
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, nested_pid, 0) as i32 };
        if raw < 0 {
            return Err("Could not retain clipboard engine process identity".into());
        }
        let pidfd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
        let socket = socket.to_path_buf();
        let display = display.to_owned();
        let authority = authority.map(std::path::Path::to_path_buf);
        let cookie = Zeroizing::new(cookie.to_vec());
        let (sender, ready) = std::sync::mpsc::sync_channel(1);
        let flag = stop.clone();
        let worker = std::thread::Builder::new()
            .name("termcraft-xdmcp-clipboard".into())
            .spawn(move || {
                let setup = (|| -> io::Result<_> {
                    let (parent, peer) = transport::parent_until(
                        &display,
                        authority.as_deref(),
                        flag.clone(),
                        deadline,
                    )?;
                    let nested = transport::connect(
                        nested,
                        0,
                        (b"MIT-MAGIC-COOKIE-1".to_vec(), cookie.to_vec()),
                        flag.clone(),
                        deadline,
                    )?;
                    Ok((
                        Endpoint::new(parent, Some(peer), deadline)?,
                        Endpoint::new(nested, None, deadline)?,
                    ))
                })();
                match setup {
                    Ok((parent, nested)) => {
                        if sender.send(Ok(())).is_err() {
                            return Ok(());
                        }
                        run(parent, nested, &flag, &socket, &cookie, nested_pid, &pidfd)
                    }
                    Err(e) => {
                        let _ = sender.send(Err(e.to_string()));
                        Ok(())
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let bridge = Self {
            stop,
            worker: Some(worker),
        };
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err("XDMCP clipboard startup cancelled".into());
            }
            if Instant::now() >= deadline {
                return Err("XDMCP clipboard startup deadline".into());
            }
            match ready.recv_timeout(Duration::from_millis(10)) {
                Ok(Ok(())) => return Ok(bridge),
                Ok(Err(e)) => return Err(format!("Could not start XDMCP text clipboard: {e}")),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("XDMCP clipboard startup worker exited".into()),
            }
        }
    }
    pub(super) fn check(&mut self) -> Result<(), String> {
        if self.worker.as_ref().is_some_and(|h| h.is_finished()) {
            return match self.worker.take().unwrap().join() {
                Ok(Err(e)) => Err(format!("XDMCP text clipboard stopped: {e}")),
                _ => Err("XDMCP text clipboard stopped".into()),
            };
        }
        Ok(())
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}
#[derive(Clone, Copy)]
struct Atoms {
    selection: u32,
    utf8: u32,
    targets: u32,
    incr: u32,
    property: u32,
    timestamp: u32,
}
struct SensitiveProperty(GetPropertyReply);
impl std::ops::Deref for SensitiveProperty {
    type Target = GetPropertyReply;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for SensitiveProperty {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for SensitiveProperty {
    fn drop(&mut self) {
        self.0.value.zeroize();
    }
}
struct Incoming {
    window: u32,
    owner: u32,
    time: u32,
    deadline: Instant,
    stage: Stage,
}
enum Stage {
    Targets,
    Text,
    Incr {
        bytes: Zeroizing<Vec<u8>>,
        minimum: usize,
    },
}
struct Outgoing {
    window: u32,
    property: u32,
    bytes: Zeroizing<Vec<u8>>,
    offset: usize,
    deadline: Instant,
}
struct Endpoint {
    conn: Conn,
    window: u32,
    atoms: Atoms,
    _peer: Option<i32>,
    owner: u32,
    time: u32,
    text: Option<Zeroizing<Vec<u8>>>,
    incoming: Option<Incoming>,
    outgoing: Vec<Outgoing>,
    epoch: u64,
    publishing: Option<Zeroizing<Vec<u8>>>,
    clearing: bool,
}
enum Change {
    Text(Zeroizing<Vec<u8>>),
    Clear,
}
struct Tick {
    external: bool,
    change: Option<Change>,
}
impl Endpoint {
    fn new(conn: Conn, peer: Option<i32>, deadline: Instant) -> io::Result<Self> {
        transport::initialize_until(&conn, deadline)?;
        conn.xfixes_query_version(2, 0)
            .map_err(error)?
            .reply()
            .map_err(error)?;
        let atom = |n: &[u8]| -> io::Result<u32> {
            Ok(conn
                .intern_atom(false, n)
                .map_err(error)?
                .reply()
                .map_err(error)?
                .atom)
        };
        let atoms = Atoms {
            selection: atom(b"CLIPBOARD")?,
            utf8: atom(b"UTF8_STRING")?,
            targets: atom(b"TARGETS")?,
            incr: atom(b"INCR")?,
            property: atom(b"_TERMCRAFT_XDMCP_TEXT")?,
            timestamp: atom(b"TIMESTAMP")?,
        };
        let window = create_window(&conn)?;
        conn.xfixes_select_selection_input(
            window,
            atoms.selection,
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )
        .map_err(error)?
        .check()
        .map_err(error)?;
        let owner = conn
            .get_selection_owner(atoms.selection)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .owner;
        let mut result = Self {
            conn,
            window,
            atoms,
            _peer: peer,
            owner,
            time: 0,
            text: None,
            incoming: None,
            outgoing: Vec::new(),
            epoch: 0,
            publishing: None,
            clearing: false,
        };
        if owner != 0 {
            result.begin(owner, 0)?;
        }
        Ok(result)
    }
    fn cancel_read(&mut self) {
        if let Some(i) = self.incoming.take() {
            let _ = self.conn.destroy_window(i.window);
        }
        self.epoch = self.epoch.wrapping_add(1);
    }
    fn begin(&mut self, owner: u32, time: u32) -> io::Result<()> {
        self.cancel_read();
        let window = create_window(&self.conn)?;
        self.conn
            .convert_selection(
                window,
                self.atoms.selection,
                self.atoms.targets,
                self.atoms.property,
                time,
            )
            .map_err(error)?
            .check()
            .map_err(error)?;
        self.incoming = Some(Incoming {
            window,
            owner,
            time,
            deadline: Instant::now() + TRANSFER_TIME,
            stage: Stage::Targets,
        });
        Ok(())
    }
    fn clear_for_recovery(&mut self, deadline: Instant) -> io::Result<()> {
        transport::begin_recovery_request(&self.conn, deadline)?;
        self.publish(Change::Clear)
    }
    fn publish(&mut self, change: Change) -> io::Result<()> {
        self.cancel_read();
        self.publishing = None;
        match change {
            Change::Clear => {
                if self
                    .conn
                    .get_selection_owner(self.atoms.selection)
                    .map_err(error)?
                    .reply()
                    .map_err(error)?
                    .owner
                    == self.window
                {
                    self.conn
                        .set_selection_owner(0u32, self.atoms.selection, self.time)
                        .map_err(error)?
                        .check()
                        .map_err(error)?;
                    self.text = None;
                    self.clearing = true;
                }
            }
            Change::Text(bytes) => {
                validate_text(&bytes)?;
                // The destination server timestamp is acquired using PropertyNotify; timestamps
                // from the source server are never compared with a different server clock.
                self.conn
                    .change_property8(
                        PropMode::REPLACE,
                        self.window,
                        self.atoms.property,
                        self.atoms.utf8,
                        &[],
                    )
                    .map_err(error)?
                    .check()
                    .map_err(error)?;
                self.publishing = Some(bytes);
            }
        }
        self.conn.flush().map_err(error)
    }
    fn tick(&mut self) -> io::Result<Tick> {
        transport::renew(&self.conn);
        let mut result = Tick {
            external: false,
            change: None,
        };
        let mut completed = None;
        for _ in 0..128 {
            let Some(event) = self.conn.poll_for_event().map_err(error)? else {
                break;
            };
            match event {
                Event::XfixesSelectionNotify(e) if e.selection == self.atoms.selection => {
                    if e.owner != 0 {
                        self.clearing = false;
                    }
                    self.owner = e.owner;
                    self.time = e.selection_timestamp;
                    if e.owner == self.window {
                        continue;
                    }
                    let expected_clear = if e.owner == 0 {
                        std::mem::take(&mut self.clearing)
                    } else {
                        false
                    };
                    if e.owner == 0 && expected_clear {
                        self.cancel_read();
                        continue;
                    }
                    self.text = None;
                    self.publishing = None;
                    self.cancel_read();
                    completed = None;
                    result.change = None;
                    result.external = true;
                    if e.owner == 0 {
                        result.change = Some(Change::Clear);
                    } else {
                        self.begin(e.owner, e.selection_timestamp)?;
                    }
                }
                Event::SelectionClear(e)
                    if e.owner == self.window && e.selection == self.atoms.selection =>
                {
                    self.text = None;
                }
                Event::SelectionRequest(e)
                    if e.owner == self.window && e.selection == self.atoms.selection =>
                {
                    self.serve(e)?;
                }
                Event::SelectionNotify(e)
                    if self
                        .incoming
                        .as_ref()
                        .is_some_and(|i| i.window == e.requestor && i.time == e.time)
                        && e.selection == self.atoms.selection =>
                {
                    if e.property != self.atoms.property {
                        self.cancel_read();
                        continue;
                    }
                    if let Some(bytes) = self.receive(false)? {
                        completed = Some((self.epoch, bytes));
                    }
                }
                Event::PropertyNotify(e)
                    if e.window == self.window
                        && e.atom == self.atoms.property
                        && e.state == Property::NEW_VALUE =>
                {
                    if let Some(bytes) = self.publishing.take() {
                        self.conn
                            .set_selection_owner(self.window, self.atoms.selection, e.time)
                            .map_err(error)?
                            .check()
                            .map_err(error)?;
                        if self
                            .conn
                            .get_selection_owner(self.atoms.selection)
                            .map_err(error)?
                            .reply()
                            .map_err(error)?
                            .owner
                            == self.window
                        {
                            self.text = Some(bytes);
                            self.owner = self.window;
                            self.time = e.time;
                        }
                    }
                }
                Event::PropertyNotify(e)
                    if e.state == Property::NEW_VALUE
                        && e.atom == self.atoms.property
                        && self.incoming.as_ref().is_some_and(|i| {
                            i.window == e.window && matches!(i.stage, Stage::Incr { .. })
                        }) =>
                {
                    if let Some(bytes) = self.receive(true)? {
                        completed = Some((self.epoch, bytes));
                    }
                }
                Event::PropertyNotify(e) if e.state == Property::DELETE => {
                    self.send_chunk(e.window, e.atom)?;
                }
                _ => {}
            }
        }
        if self
            .incoming
            .as_ref()
            .is_some_and(|i| Instant::now() >= i.deadline)
        {
            self.cancel_read();
        }
        for index in (0..self.outgoing.len()).rev() {
            if Instant::now() >= self.outgoing[index].deadline {
                self.finish_outgoing(index)?;
            }
        }
        if let Some((epoch, bytes)) = completed {
            if self.epoch == epoch {
                result.change = Some(Change::Text(bytes));
            }
        }
        self.conn.flush().map_err(error)?;
        Ok(result)
    }
    fn receive(&mut self, chunk: bool) -> io::Result<Option<Zeroizing<Vec<u8>>>> {
        let Some(mut i) = self.incoming.take() else {
            return Ok(None);
        };
        let mut property = SensitiveProperty(
            self.conn
                .get_property(
                    true,
                    i.window,
                    self.atoms.property,
                    AtomEnum::ANY,
                    0,
                    (MAX_TEXT / 4 + 1) as u32,
                )
                .map_err(error)?
                .reply()
                .map_err(error)?,
        );
        let valid = property.bytes_after == 0 && property.value.len() <= MAX_TEXT;
        if !valid {
            let _ = self.conn.destroy_window(i.window);
            return Ok(None);
        }
        match &mut i.stage {
            Stage::Targets if !chunk => {
                if property.type_ != u32::from(AtomEnum::ATOM)
                    || property.format != 32
                    || !property
                        .value32()
                        .is_some_and(|mut a| a.any(|a| a == self.atoms.utf8))
                {
                    let _ = self.conn.destroy_window(i.window);
                    return Ok(None);
                }
                self.conn
                    .convert_selection(
                        i.window,
                        self.atoms.selection,
                        self.atoms.utf8,
                        self.atoms.property,
                        i.time,
                    )
                    .map_err(error)?
                    .check()
                    .map_err(error)?;
                i.stage = Stage::Text;
                self.incoming = Some(i);
            }
            Stage::Text if !chunk && property.type_ == self.atoms.incr => {
                if property.format != 32
                    || property.value32().is_none_or(|mut v| {
                        v.next().is_none_or(|n| n as usize > MAX_TEXT) || v.next().is_some()
                    })
                {
                    let _ = self.conn.destroy_window(i.window);
                    return Ok(None);
                }
                let minimum = property.value32().and_then(|mut v| v.next()).unwrap() as usize;
                i.stage = Stage::Incr {
                    bytes: Zeroizing::new(Vec::new()),
                    minimum,
                };
                self.incoming = Some(i);
            }
            Stage::Text if !chunk => {
                let owner = self
                    .conn
                    .get_selection_owner(self.atoms.selection)
                    .map_err(error)?
                    .reply()
                    .map_err(error)?
                    .owner;
                let _ = self.conn.destroy_window(i.window);
                if owner == i.owner
                    && property.type_ == self.atoms.utf8
                    && property.format == 8
                    && validate_text(&property.value).is_ok()
                {
                    return Ok(Some(Zeroizing::new(std::mem::take(&mut property.value))));
                }
            }
            Stage::Incr { bytes, minimum } if chunk => {
                if property.type_ != self.atoms.utf8
                    || property.format != 8
                    || bytes.len() + property.value.len() > MAX_TEXT
                {
                    let _ = self.conn.destroy_window(i.window);
                    return Ok(None);
                }
                if property.value.is_empty() {
                    let owner = self
                        .conn
                        .get_selection_owner(self.atoms.selection)
                        .map_err(error)?
                        .reply()
                        .map_err(error)?
                        .owner;
                    let _ = self.conn.destroy_window(i.window);
                    if owner == i.owner && validate_incr_complete(bytes, *minimum).is_ok() {
                        return Ok(Some(std::mem::take(bytes)));
                    }
                } else {
                    bytes.extend_from_slice(&property.value);
                    self.incoming = Some(i);
                }
            }
            _ => {
                let _ = self.conn.destroy_window(i.window);
            }
        }
        Ok(None)
    }
    fn serve(&mut self, e: SelectionRequestEvent) -> io::Result<()> {
        let property = if e.property == 0 {
            e.target
        } else {
            e.property
        };
        let mut accepted = 0;
        if let Some(bytes) = self.text.as_ref() {
            if e.time == 0 || e.time.wrapping_sub(self.time) < 0x8000_0000 {
                let stored = if e.target == self.atoms.targets {
                    self.conn
                        .change_property32(
                            PropMode::REPLACE,
                            e.requestor,
                            property,
                            AtomEnum::ATOM,
                            &[self.atoms.targets, self.atoms.utf8, self.atoms.timestamp],
                        )
                        .map_err(error)?
                        .check()
                        .is_ok()
                } else if e.target == self.atoms.timestamp {
                    self.conn
                        .change_property32(
                            PropMode::REPLACE,
                            e.requestor,
                            property,
                            AtomEnum::INTEGER,
                            &[self.time],
                        )
                        .map_err(error)?
                        .check()
                        .is_ok()
                } else if e.target == self.atoms.utf8 && bytes.len() <= CHUNK {
                    self.conn
                        .change_property8(
                            PropMode::REPLACE,
                            e.requestor,
                            property,
                            self.atoms.utf8,
                            bytes,
                        )
                        .map_err(error)?
                        .check()
                        .is_ok()
                } else if e.target == self.atoms.utf8
                    && self.outgoing.len() < 4
                    && self.outgoing.iter().map(|o| o.bytes.len()).sum::<usize>() + bytes.len()
                        <= MAX_TEXT
                    && !self
                        .outgoing
                        .iter()
                        .any(|o| o.window == e.requestor && o.property == property)
                {
                    // At most four snapshots and 1 MiB total outgoing state plus one incoming.
                    if self
                        .conn
                        .change_window_attributes(
                            e.requestor,
                            &ChangeWindowAttributesAux::new()
                                .event_mask(EventMask::PROPERTY_CHANGE),
                        )
                        .map_err(error)?
                        .check()
                        .is_ok()
                        && self
                            .conn
                            .change_property32(
                                PropMode::REPLACE,
                                e.requestor,
                                property,
                                self.atoms.incr,
                                &[bytes.len() as u32],
                            )
                            .map_err(error)?
                            .check()
                            .is_ok()
                    {
                        self.outgoing.push(Outgoing {
                            window: e.requestor,
                            property,
                            bytes: bytes.clone(),
                            offset: 0,
                            deadline: Instant::now() + TRANSFER_TIME,
                        });
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if stored {
                    accepted = property;
                }
            }
        }
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property: accepted,
        };
        // A requestor may disappear at any time: BadWindow fails this request only.
        let _ = self
            .conn
            .send_event(false, e.requestor, EventMask::NO_EVENT, notify)
            .map_err(error)?
            .check();
        Ok(())
    }
    fn send_chunk(&mut self, window: u32, property: u32) -> io::Result<()> {
        let Some(index) = self
            .outgoing
            .iter()
            .position(|o| o.window == window && o.property == property)
        else {
            return Ok(());
        };
        let o = &mut self.outgoing[index];
        let end = (o.offset + CHUNK).min(o.bytes.len());
        let success = self
            .conn
            .change_property8(
                PropMode::REPLACE,
                window,
                property,
                self.atoms.utf8,
                &o.bytes[o.offset..end],
            )
            .map_err(error)?
            .check()
            .is_ok();
        if !success || o.offset == o.bytes.len() {
            self.finish_outgoing(index)?;
        } else {
            o.offset = end;
        }
        Ok(())
    }
    fn finish_outgoing(&mut self, index: usize) -> io::Result<()> {
        let outgoing = self.outgoing.swap_remove(index);
        if !self.outgoing.iter().any(|o| o.window == outgoing.window) {
            // Remove this connection's event subscription when the bounded transfer ends.
            let _ = self
                .conn
                .change_window_attributes(
                    outgoing.window,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
                )
                .map_err(error)?
                .check();
        }
        Ok(())
    }
}
// Dropping the connection destroys only this client's selection/request windows.
fn create_window(conn: &Conn) -> io::Result<u32> {
    let window = conn.generate_id().map_err(error)?;
    let root = conn
        .setup()
        .roots
        .first()
        .ok_or_else(|| error("X11 screen unavailable"))?
        .root;
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
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .map_err(error)?
    .check()
    .map_err(error)?;
    Ok(window)
}
fn run(
    mut parent: Endpoint,
    mut nested: Endpoint,
    stop: &Arc<AtomicBool>,
    socket: &std::path::Path,
    cookie: &[u8],
    pid: u32,
    pidfd: &std::os::fd::OwnedFd,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let mut resets = std::collections::VecDeque::new();
    let mut nested_error = None;
    while !stop.load(Ordering::Relaxed) {
        let a = parent.tick()?;
        let tick = match nested_error.take() {
            Some(e) => Err(e),
            None => nested.tick(),
        };
        let b = match tick {
            Ok(tick) => tick,
            Err(e) => {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let now = Instant::now();
                while resets
                    .front()
                    .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(30))
                {
                    resets.pop_front();
                }
                if resets.len() >= 3 {
                    return Err(error(format!("Nested clipboard reset limit reached: {e}")));
                }
                resets.push_back(now);
                // xdm SecureDisplay/pseudoReset kills clients with windows. A fresh
                // connection is permitted only while the retained child pidfd is live,
                // with a new SO_PEERCRED check before sending the private cookie.
                let live = || -> io::Result<()> {
                    let mut fd = libc::pollfd {
                        fd: pidfd.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    if unsafe { libc::poll(&mut fd, 1, 0) } != 0 {
                        return Err(error("Owned clipboard engine exited"));
                    }
                    Ok(())
                };
                live()?;
                let recovery_deadline = now + Duration::from_secs(2);
                parent.clear_for_recovery(recovery_deadline)?;
                let stream =
                    super::xdmcp_engine::connect_ready_socket(socket, recovery_deadline, stop)
                        .map_err(error)?;
                if transport::peer(&stream)?.pid != pid as i32 {
                    return Err(error("Nested clipboard reset peer identity mismatch"));
                }
                live()?;
                let conn = transport::connect(
                    stream,
                    0,
                    (b"MIT-MAGIC-COOKIE-1".to_vec(), cookie.to_vec()),
                    stop.clone(),
                    recovery_deadline,
                )?;
                nested = Endpoint::new(conn, None, recovery_deadline)?;
                // Resample only a current external parent selection. Old nested text
                // is never carried across a display-manager reset/login boundary.
                transport::begin_recovery_request(&parent.conn, recovery_deadline)?;
                let owner = parent
                    .conn
                    .get_selection_owner(parent.atoms.selection)
                    .map_err(error)?
                    .reply()
                    .map_err(error)?
                    .owner;
                if owner != 0 && owner != parent.window {
                    parent.begin(owner, 0)?;
                }
                continue;
            }
        };
        // A new external selection invalidates the opposite outstanding epoch.
        // If both servers change in one observation round, the nested change wins;
        // cancelling both would lose both conversions with no future notification.
        if b.external {
            parent.cancel_read();
            parent.publishing = None;
            if let Some(c) = b.change {
                parent.publish(c)?;
            }
        } else if a.external {
            nested.cancel_read();
            nested.publishing = None;
            if let Some(c) = a.change {
                if let Err(e) = nested.publish(c) {
                    nested_error = Some(e);
                }
            }
        } else if let Some(c) = b.change {
            parent.publish(c)?;
        } else if let Some(c) = a.change {
            if let Err(e) = nested.publish(c) {
                nested_error = Some(e);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn selected_display_is_local_and_bounded() {
        for good in [":0", ":177.0", ":59535.255"] {
            assert!(super::validate_display(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "localhost:0",
            "tcp/host:0",
            ":59536",
            ":0.256",
            ":0.",
            ":-1",
            ":0.0.1",
        ] {
            assert!(super::validate_display(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn incr_completion_rejects_truncated_utf8_text() {
        assert!(super::validate_incr_complete("🐧\n".as_bytes(), 5).is_ok());
        assert!(super::validate_incr_complete("🐧\n".as_bytes(), 6).is_err());
        assert!(super::validate_incr_complete(&[255], 1).is_err());
    }
    #[test]
    fn bounded_text_is_utf8_only() {
        assert!(super::validate_text(b"hello \xf0\x9f\x90\xa7\nworld").is_ok());
        assert!(super::validate_text(b"bad\0text").is_err());
        assert!(super::validate_text(&[255]).is_err());
        assert!(super::validate_text(&vec![b'x'; super::MAX_TEXT + 1]).is_err());
    }
}

#[cfg(test)]
mod fixture;

#[cfg(test)]
pub(super) fn verify_managed(
    session: &super::xdmcp_engine::XdmcpSession,
    authority: &std::path::Path,
) {
    fixture::verify_managed(session, authority)
}

#[cfg(test)]
mod deadline_tests;
