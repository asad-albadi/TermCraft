use super::{
    ndr::*,
    pcsc::{self, Api, Handle},
};
use std::{
    collections::BTreeMap,
    ffi::CString,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const INVALID_HANDLE: u32 = 0x80100003;
const INVALID_PARAMETER: u32 = 0x80100004;
const CANCELLED: u32 = 0x80100002;
const UNKNOWN_READER: u32 = 0x80100009;
const TIMEOUT: u32 = 0x8010000a;
const PROTOCOL_MISMATCH: u32 = 0x8010000f;
const NO_MEMORY: u32 = 0x80100006;
const UNSUPPORTED_FEATURE: u32 = 0x80100022;
const ACCESS_STARTED_EVENT: u32 = 0x0009_00e0;
const LEAVE_CARD: u32 = 0;
const MAX_CONTEXTS: usize = 16;
const MAX_CARDS: usize = 16;
const MAX_WATCHES: usize = 8;
const MAX_WATCH_RETURN_READERS: usize = 10;
const PNP_NOTIFICATION: &str = "\\\\?PnP?\\Notification";

type Reply = Zeroizing<Vec<u8>>;

#[derive(Clone, Copy)]
struct Request {
    header: [u8; 24],
    output: usize,
}
impl Request {
    fn parse(bytes: &[u8]) -> Result<(Self, u32, &[u8]), u32> {
        if bytes.len() > MAX_PACKET
            || !bytes.starts_with(b"rDRI")
            || word_at(bytes, 16)? != 0xe
            || word_at(bytes, 20)? != 0
        {
            return Err(INVALID);
        }
        let payload = bytes.get(56..).ok_or(INVALID)?;
        if word_at(bytes, 28)? as usize != payload.len() {
            return Err(INVALID);
        }
        let header = bytes[..24].try_into().unwrap();
        let output = (word_at(bytes, 24)? as usize).min(MAX_PACKET);
        if output < 24 {
            return Err(SMALL);
        }
        Ok((Self { header, output }, word_at(bytes, 32)?, payload))
    }

    fn output(self, needed: usize) -> Result<(), u32> {
        if 16 + needed.next_multiple_of(8) > self.output {
            Err(SMALL)
        } else {
            Ok(())
        }
    }

    fn value(self, capacity: usize, result: u32, write: impl FnOnce(&mut Out)) -> Reply {
        let mut output = Out::response(&self.header, capacity);
        output.word(result);
        write(&mut output);
        output.finish(self.output)
    }

    fn long(self, result: u32) -> Reply {
        self.value(4, result, |_| {})
    }
}

enum Call<'a> {
    Ready,
    Establish(u32),
    Context(u32, ContextAction),
    Readers {
        context: u32,
        groups: bool,
        wide: bool,
        lengths_only: bool,
        capacity: u32,
    },
    Connect {
        context: u32,
        reader: String,
        share: u32,
        protocols: u32,
    },
    Watch {
        context: u32,
        timeout: u32,
        readers: Vec<WatchedReader>,
    },
    Card {
        key: (u32, u32),
        action: CardAction,
        disposition: u32,
    },
    Status {
        key: (u32, u32),
        wide: bool,
        lengths_only: bool,
        name_capacity: u32,
    },
    Transmit {
        key: (u32, u32),
        protocol: u32,
        apdu: &'a [u8],
        recv_pci: bool,
        lengths_only: bool,
        capacity: usize,
    },
}

enum ContextAction {
    Release,
    Validate,
    Cancel,
}
enum CardAction {
    Disconnect,
    Begin,
    End,
}

struct WatchedReader {
    name: String,
    state: u32,
}

fn parse_call(code: u32, data: &[u8]) -> Result<Call<'_>, u32> {
    if code == ACCESS_STARTED_EVENT {
        return if data.len() == 4 {
            Ok(Call::Ready)
        } else {
            Err(INVALID)
        };
    }
    if !matches!(code, 0x90014..=0x9002c | 0x900a0..=0x900b0 | 0x900b8..=0x900c0 | 0x900c8..=0x900d0)
        || code % 4 != 0
    {
        return Err(UNSUPPORTED);
    }
    let mut ndr = In::new(data)?;
    let call = match code {
        0x90014 => Call::Establish(ndr.word()?),
        0x90018 | 0x9001c | 0x900a8 => {
            ndr.token_header()?;
            let context = ndr.token()?;
            let action = match code {
                0x90018 => ContextAction::Release,
                0x9001c => ContextAction::Validate,
                _ => ContextAction::Cancel,
            };
            Call::Context(context, action)
        }
        0x90020 | 0x90024 | 0x90028 | 0x9002c => {
            ndr.token_header()?;
            let groups = matches!(code, 0x90020 | 0x90024);
            let wide = matches!(code, 0x90024 | 0x9002c);
            let groups_len = if groups { 0 } else { ndr.count(65_536)? };
            let groups_present = !groups && ndr.pointer()?;
            let lengths_only = ndr.boolean()?;
            let capacity = ndr.word()?;
            let context = ndr.token()?;
            if groups_present {
                ndr.conformant(groups_len)?;
            } else if groups_len != 0 {
                return Err(INVALID);
            }
            Call::Readers {
                context,
                groups,
                wide,
                lengths_only,
                capacity,
            }
        }
        0x900a0 | 0x900a4 => {
            ndr.token_header()?;
            let timeout = ndr.word()?;
            let count = ndr.count(11)?;
            let present = ndr.pointer()?;
            let context = ndr.token()?;
            if count != 0 && !present || present && ndr.word()? as usize != count {
                return Err(INVALID);
            }
            let mut states = Vec::with_capacity(count);
            for _ in 0..count {
                if !ndr.pointer()? {
                    return Err(INVALID);
                }
                states.push(ndr.word()?);
                ndr.word()?; // event-state input ignored by PC/SC
                ndr.count(36)?;
                ndr.take(36)?;
            }
            let mut readers = Vec::with_capacity(count);
            for state in states {
                readers.push(WatchedReader {
                    state,
                    name: ndr.reader(code == 0x900a4)?,
                });
            }
            Call::Watch {
                context,
                timeout,
                readers,
            }
        }
        0x900ac | 0x900b0 => {
            if !ndr.pointer()? {
                return Err(INVALID);
            }
            ndr.token_header()?;
            let share = ndr.word()?;
            let protocols = ndr.word()?;
            let reader = ndr.reader(code == 0x900b0)?;
            let context = ndr.token()?;
            Call::Connect {
                context,
                reader,
                share,
                protocols,
            }
        }
        0x900b8 | 0x900bc | 0x900c0 => {
            ndr.card_header()?;
            let disposition = ndr.word()?;
            let key = ndr.card()?;
            let action = match code {
                0x900b8 => CardAction::Disconnect,
                0x900bc => CardAction::Begin,
                _ => CardAction::End,
            };
            Call::Card {
                key,
                action,
                disposition,
            }
        }
        0x900c8 | 0x900cc => {
            ndr.card_header()?;
            let lengths_only = ndr.boolean()?;
            let name_capacity = ndr.word()?;
            ndr.word()?; // cbAtrLen is unused in MS-RDPESC Status_Call.
            let key = ndr.card()?;
            Call::Status {
                key,
                wide: code == 0x900cc,
                lengths_only,
                name_capacity,
            }
        }
        0x900d0 => {
            ndr.card_header()?;
            let protocol = ndr.word()?;
            let extra = ndr.count(1024)?;
            let extra_present = ndr.pointer()?;
            let apdu_len = ndr.count(MAX_APDU as u32)?;
            let apdu_present = ndr.pointer()?;
            let recv_pci = ndr.pointer()?;
            let lengths_only = ndr.boolean()?;
            let capacity = ndr.word()? as usize;
            let key = ndr.card()?;
            if extra_present {
                ndr.conformant(extra)?;
            } else if extra != 0 {
                return Err(INVALID);
            }
            let apdu = if apdu_present {
                ndr.conformant(apdu_len)?
            } else {
                &[]
            };
            if apdu_len != apdu.len() {
                return Err(INVALID);
            }
            let mut recv_extra = 0;
            if recv_pci {
                ndr.word()?; // receiver's protocol input is not used
                recv_extra = ndr.count(1024)?;
                if ndr.pointer()? {
                    ndr.conformant(recv_extra)?;
                } else if recv_extra != 0 {
                    return Err(INVALID);
                }
            }
            if extra != 0 || recv_extra != 0 {
                return Err(UNSUPPORTED);
            }
            Call::Transmit {
                key,
                protocol,
                apdu,
                recv_pci,
                lengths_only,
                capacity,
            }
        }
        _ => return Err(UNSUPPORTED),
    };
    ndr.finish()?;
    Ok(call)
}

struct Card {
    native: Handle,
    protocol: u32,
}
struct Context {
    native: Handle,
    cards: BTreeMap<u32, Card>,
}
struct Watch {
    request: Request,
    context: u32,
    until: Option<Instant>,
    readers: Vec<WatchedReader>,
}

pub(super) struct Session {
    api: Api,
    reader: CString,
    contexts: BTreeMap<u32, Context>,
    seq: u32,
    watches: Vec<Watch>,
}

impl Session {
    pub fn open(reader: CString, api: Api) -> Result<Self, u32> {
        let context = api.establish()?;
        let found = api.contains_reader(context, &reader);
        api.release(context)?;
        if !found? {
            return Err(UNKNOWN_READER);
        }
        Ok(Self {
            api,
            reader,
            contexts: BTreeMap::new(),
            seq: 0,
            watches: Vec::new(),
        })
    }

    pub fn process(&mut self, bytes: &[u8]) -> Vec<Reply> {
        if bytes.len() < 24 || unassigned_control(bytes) {
            return Vec::new();
        }
        let result = Request::parse(bytes).and_then(|(request, code, input)| {
            let call = parse_call(code, input)?;
            self.handle(request, call)
        });
        result.unwrap_or_else(|status| vec![fail(bytes, status)])
    }

    fn handle(&mut self, request: Request, call: Call<'_>) -> Result<Vec<Reply>, u32> {
        let response = match call {
            Call::Ready => request.long(0),
            Call::Establish(scope) => {
                request.output(20)?;
                let result = self.establish(scope);
                request.value(20, result.as_ref().err().copied().unwrap_or(0), |out| {
                    out.token_header(result.is_ok());
                    if let Ok(context) = result {
                        out.token(context);
                    }
                })
            }
            Call::Context(context, action) => {
                let native = match self.native_context(context) {
                    Ok(handle) => handle,
                    Err(error) => return Ok(vec![request.long(error)]),
                };
                let result = match action {
                    ContextAction::Validate => self.api.valid(native),
                    ContextAction::Cancel => self.api.cancel(native),
                    ContextAction::Release => self.api.release(native),
                };
                if result.is_ok() && matches!(action, ContextAction::Release) {
                    self.contexts.remove(&context);
                }
                let retire_watches = result.is_ok() && !matches!(action, ContextAction::Validate);
                let mut replies = vec![request.long(result.err().unwrap_or(0))];
                if retire_watches {
                    self.watches.retain(|watch| {
                        if watch.context == context {
                            replies.push(watch_reply(watch.request, CANCELLED, &[]));
                            false
                        } else {
                            true
                        }
                    });
                }
                return Ok(replies);
            }
            Call::Readers {
                context,
                groups,
                wide,
                lengths_only,
                capacity,
            } => self.list(request, context, groups, wide, lengths_only, capacity),
            Call::Connect {
                context,
                reader,
                share,
                protocols,
            } => {
                request.output(40)?;
                let result = self.connect(context, &reader, share, protocols);
                request.value(40, result.as_ref().err().copied().unwrap_or(0), |out| {
                    out.token_header(result.is_ok());
                    out.token_header(result.is_ok());
                    out.word(result.as_ref().map(|(_, protocol)| *protocol).unwrap_or(0));
                    if let Ok((card, _)) = result {
                        out.token(context);
                        out.token(card);
                    }
                })
            }
            Call::Watch {
                context,
                timeout,
                readers,
            } => {
                let watch = Watch {
                    request,
                    context,
                    until: (timeout != u32::MAX)
                        .then(|| Instant::now() + Duration::from_millis(timeout.into())),
                    readers,
                };
                if let Some(response) = self.watch(&watch) {
                    response
                } else if self.watches.len() == MAX_WATCHES {
                    watch_reply(request, NO_MEMORY, &[])
                } else {
                    self.watches.push(watch);
                    return Ok(Vec::new());
                }
            }
            Call::Card {
                key,
                action,
                disposition,
            } => request.long(
                self.card_action(key, action, disposition)
                    .err()
                    .unwrap_or(0),
            ),
            Call::Status {
                key,
                wide,
                lengths_only,
                name_capacity,
            } => self.status(request, key, wide, lengths_only, name_capacity),
            Call::Transmit {
                key,
                protocol,
                apdu,
                recv_pci,
                lengths_only,
                capacity,
            } => self.transmit(
                request,
                key,
                protocol,
                apdu,
                recv_pci,
                lengths_only,
                capacity,
            )?,
        };
        Ok(vec![response])
    }

    fn next(&mut self) -> Result<u32, u32> {
        self.seq = self.seq.checked_add(1).ok_or(NO_MEMORY)?;
        Ok(self.seq)
    }

    fn native_context(&self, token: u32) -> Result<Handle, u32> {
        Ok(self.contexts.get(&token).ok_or(INVALID_HANDLE)?.native)
    }

    fn card(&self, key: (u32, u32)) -> Result<&Card, u32> {
        self.contexts
            .get(&key.0)
            .and_then(|context| context.cards.get(&key.1))
            .ok_or(INVALID_HANDLE)
    }

    fn establish(&mut self, scope: u32) -> Result<u32, u32> {
        if scope > 2 {
            return Err(INVALID_PARAMETER);
        }
        if self.contexts.len() >= MAX_CONTEXTS {
            return Err(NO_MEMORY);
        }
        let token = self.next()?;
        let native = self.api.establish()?;
        self.contexts.insert(
            token,
            Context {
                native,
                cards: BTreeMap::new(),
            },
        );
        Ok(token)
    }

    fn reader_name(&self) -> &str {
        // The child accepts only a UTF-8 reader selected explicitly by the caller.
        self.reader.to_str().unwrap()
    }

    fn list(
        &self,
        request: Request,
        context: u32,
        groups: bool,
        wide: bool,
        lengths_only: bool,
        capacity: u32,
    ) -> Reply {
        let result = self.native_context(context).and_then(|native| {
            let name = if groups {
                self.api.valid(native)?;
                "SCard$DefaultReaders"
            } else if self.api.contains_reader(native, &self.reader)? {
                self.reader_name()
            } else {
                return Err(pcsc::NO_READERS);
            };
            multistring(name, wide)
        });
        let mut status = result.as_ref().err().copied().unwrap_or(0);
        let names = result.unwrap_or_default();
        let deliver = status == 0 && !lengths_only && capacity != 0;
        let chars = names.len() / if wide { 2 } else { 1 };
        if deliver && chars > capacity as usize {
            status = pcsc::INSUFFICIENT;
        }
        let include = deliver && status == 0;
        request.value(16 + names.len(), status, |out| {
            out.word(names.len() as u32);
            out.pointer(include);
            if include {
                out.word(names.len() as u32);
                out.bytes(&names);
            }
        })
    }

    fn connect(
        &mut self,
        token: u32,
        reader: &str,
        share: u32,
        protocols: u32,
    ) -> Result<(u32, u32), u32> {
        let context = self.native_context(token)?;
        if reader != self.reader_name() {
            return Err(UNKNOWN_READER);
        }
        // Shared/exclusive T=0 or T=1. Raw/direct mode is reader-driver specific.
        if !matches!(share, 1 | 2) || protocols & !0x8000_0003 != 0 {
            return Err(UNSUPPORTED_FEATURE);
        }
        if self.contexts.values().map(|c| c.cards.len()).sum::<usize>() >= MAX_CARDS {
            return Err(NO_MEMORY);
        }
        let protocols = match protocols & 3 {
            0 => 3,
            value => value,
        };
        let card_token = self.next()?;
        let (native, protocol) = self.api.connect(context, &self.reader, share, protocols)?;
        self.contexts
            .get_mut(&token)
            .unwrap()
            .cards
            .insert(card_token, Card { native, protocol });
        Ok((card_token, protocol))
    }

    fn card_action(
        &mut self,
        key: (u32, u32),
        action: CardAction,
        disposition: u32,
    ) -> Result<(), u32> {
        let card = self.card(key)?.native;
        if disposition > 3 && !matches!(action, CardAction::Begin) {
            return Err(INVALID_PARAMETER);
        }
        match action {
            CardAction::Disconnect => {
                self.api.disconnect(card, disposition)?;
                self.contexts.get_mut(&key.0).unwrap().cards.remove(&key.1);
                Ok(())
            }
            CardAction::Begin => self.api.begin(card),
            CardAction::End => self.api.end(card, disposition),
        }
    }

    fn status(
        &self,
        request: Request,
        key: (u32, u32),
        wide: bool,
        lengths_only: bool,
        name_capacity: u32,
    ) -> Reply {
        let result = self
            .card(key)
            .and_then(|card| self.api.status(card.native))
            .and_then(|status| Ok((status, multistring(self.reader_name(), wide)?)));
        match result {
            Ok((status, names)) => {
                let chars = names.len() / if wide { 2 } else { 1 };
                let short_names =
                    !lengths_only && name_capacity != 0 && chars > name_capacity as usize;
                let short_atr = status.atr_len > 32;
                let include = !lengths_only && name_capacity != 0 && !short_names && !short_atr;
                let error = if short_names || short_atr {
                    pcsc::INSUFFICIENT
                } else {
                    0
                };
                request.value(60 + names.len(), error, |out| {
                    out.word(names.len() as u32);
                    out.pointer(include);
                    out.word(status.state);
                    out.word(status.protocol);
                    out.bytes(&status.atr[..32]);
                    out.word(status.atr_len.min(32) as u32);
                    if include {
                        out.word(names.len() as u32);
                        out.bytes(&names);
                    }
                })
            }
            Err(error) => request.value(56, error, |out| out.bytes(&[0; 52])),
        }
    }

    fn transmit(
        &self,
        request: Request,
        key: (u32, u32),
        protocol: u32,
        apdu: &[u8],
        recv_pci: bool,
        lengths_only: bool,
        capacity: usize,
    ) -> Result<Reply, u32> {
        let fixed_output = if recv_pci { 28 } else { 16 };
        request.output(fixed_output)?;
        if !lengths_only && capacity <= MAX_APDU && capacity != 0 {
            // Reject inconsistent outer framing before dispatching an APDU.
            request.output(fixed_output + 4 + capacity)?;
        }
        let result = self.card(key).and_then(|card| {
            if card.protocol != protocol || !matches!(protocol, 1 | 2) {
                return Err(PROTOCOL_MISMATCH);
            }
            if apdu.is_empty() || (!lengths_only && capacity > MAX_APDU) {
                return Err(INVALID_PARAMETER);
            }
            let capacity = if lengths_only { MAX_APDU } else { capacity };
            self.api
                .transmit(card.native, protocol, apdu, recv_pci, capacity)
        });
        let status = result.as_ref().err().copied().unwrap_or(0);
        let include_pci = result.is_ok() && recv_pci;
        let (protocol, bytes) = result.unwrap_or_default();
        let include_buffer = !lengths_only && !bytes.is_empty();
        let reserved = fixed_output + if include_buffer { 4 + bytes.len() } else { 0 };
        Ok(request.value(reserved, status, |out| {
            out.pointer(include_pci);
            out.word(bytes.len() as u32);
            out.pointer(include_buffer);
            if include_pci {
                out.word(protocol);
                out.word(0);
                out.pointer(false);
            }
            if include_buffer {
                out.word(bytes.len() as u32);
                out.bytes(&bytes);
            }
        }))
    }

    fn watch(&self, watch: &Watch) -> Option<Reply> {
        let context = match self.native_context(watch.context) {
            Ok(context) => context,
            Err(error) => return Some(watch_reply(watch.request, error, &[])),
        };
        // The request permits 11 entries, but the return structure permits only 10.
        if watch.readers.is_empty() || watch.readers.len() > MAX_WATCH_RETURN_READERS {
            return Some(watch_reply(watch.request, INVALID_PARAMETER, &[]));
        }
        let mut changed = false;
        let mut states = Vec::with_capacity(watch.readers.len());
        for reader in &watch.readers {
            let (event, mut atr, atr_len) = if reader.state & 1 != 0 {
                (1, [0; pcsc::ATR_MAX], 0)
            } else if reader.name == self.reader_name() {
                match self.api.watch(context, &self.reader, reader.state & !2) {
                    Ok(ready) => (ready.event, ready.atr, ready.atr_len),
                    Err(TIMEOUT) => (reader.state & !2, [0; pcsc::ATR_MAX], 0),
                    Err(error) => return Some(watch_reply(watch.request, error, &[])),
                }
            } else {
                let base = if reader.name == PNP_NOTIFICATION {
                    match self.api.contains_reader(context, &self.reader) {
                        Ok(true) => 0x0001_0000,
                        Ok(false) | Err(pcsc::NO_READERS) => 0,
                        Err(error) => return Some(watch_reply(watch.request, error, &[])),
                    }
                } else {
                    0x0004 // SCARD_STATE_UNKNOWN. Never query that reader.
                };
                let event = base | if reader.state & !2 != base { 2 } else { 0 };
                (event, [0; pcsc::ATR_MAX], 0)
            };
            atr[atr_len..].fill(0);
            changed |= event & 2 != 0;
            states.push((reader.state, event, atr, atr_len));
        }
        if changed {
            Some(watch_reply(watch.request, 0, &states))
        } else if watch.until.is_some_and(|until| Instant::now() >= until) {
            Some(watch_reply(watch.request, TIMEOUT, &[]))
        } else {
            None
        }
    }

    pub fn has_watches(&self) -> bool {
        !self.watches.is_empty()
    }

    pub fn poll_watches(&mut self) -> Vec<Reply> {
        let watches = std::mem::take(&mut self.watches);
        let mut replies = Vec::new();
        for watch in watches {
            if let Some(reply) = self.watch(&watch) {
                replies.push(reply);
            } else {
                self.watches.push(watch);
            }
        }
        replies
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for context in self.contexts.values() {
            for card in context.cards.values() {
                let _ = self.api.disconnect(card.native, LEAVE_CARD);
            }
            let _ = self.api.release(context.native);
        }
    }
}

fn multistring(name: &str, wide: bool) -> Result<Vec<u8>, u32> {
    if wide {
        Ok(name
            .encode_utf16()
            .chain([0, 0])
            .flat_map(u16::to_le_bytes)
            .collect())
    } else if name.is_ascii() {
        Ok(name.bytes().chain([0, 0]).collect())
    } else {
        Err(UNSUPPORTED_FEATURE)
    }
}

fn watch_reply(
    request: Request,
    status: u32,
    states: &[(u32, u32, [u8; pcsc::ATR_MAX], usize)],
) -> Reply {
    request.value(16 + states.len() * 48, status, |out| {
        out.word(states.len() as u32);
        out.pointer(!states.is_empty());
        if !states.is_empty() {
            out.word(states.len() as u32);
            for (current, event, atr, len) in states {
                out.word(*current);
                out.word(*event);
                out.word(*len as u32);
                out.bytes(atr);
                out.bytes(&[0; 36 - pcsc::ATR_MAX]);
            }
        }
    })
}
