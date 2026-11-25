//! Confirmation and pacing are state transitions; callers own transport writes and clocks.
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
const MAX_PASTE_BYTES: usize = 4 * 1024 * 1024;
const CHUNK: usize = 16384;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PasteSettings {
    pub confirm_multiline: bool,
    pub line_delay_ms: u32,
    #[serde(skip)]
    pub bracketed: bool,
}
impl Default for PasteSettings {
    fn default() -> Self {
        Self {
            confirm_multiline: true,
            line_delay_ms: 0,
            bracketed: false,
        }
    }
}
pub enum PasteDecision {
    Send(PasteController),
    Confirm(PasteController),
    Reject(String),
}
pub struct PasteController {
    text: String,
    cursor: usize,
    settings: PasteSettings,
    approved: bool,
    done: bool,
    opened: bool,
    next: Option<Instant>,
    checkpoint: Option<(usize, bool, bool, Option<Instant>)>,
}
impl PasteController {
    pub fn begin(text: String, mut settings: PasteSettings) -> PasteDecision {
        if text.len() > MAX_PASTE_BYTES {
            return PasteDecision::Reject(
                "Paste exceeds 4 MiB. Paste a smaller selection or transfer a file.".into(),
            );
        }
        if text.contains("\x1b[200~") || text.contains("\x1b[201~") {
            return PasteDecision::Reject("Paste contains a bracketed-paste control delimiter. Remove the control sequence before pasting.".into());
        }
        settings.line_delay_ms = settings.line_delay_ms.min(5000);
        let confirm = settings.confirm_multiline && text.contains(['\n', '\r']);
        let controller = Self {
            text,
            cursor: 0,
            settings,
            approved: !confirm,
            done: false,
            opened: false,
            next: None,
            checkpoint: None,
        };
        if confirm {
            PasteDecision::Confirm(controller)
        } else {
            PasteDecision::Send(controller)
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn preview(&self) -> String {
        self.text
            .chars()
            .take(2000)
            .flat_map(|c| {
                if c.is_control() && c != '\n' && c != '\t' {
                    c.escape_default().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .collect()
    }
    pub fn awaiting_confirmation(&self) -> bool {
        !self.approved && !self.done
    }
    pub fn confirm_for_terminal(&mut self, now: Instant, bracketed: bool) {
        if self.cursor == 0 && !self.opened && !self.done {
            self.settings.bracketed = bracketed;
        }
        self.confirm(now);
    }
    pub fn confirm(&mut self, now: Instant) {
        if !self.done {
            self.approved = true;
            self.next = Some(now);
        }
    }
    pub fn finished(&self) -> bool {
        self.done
    }
    pub fn tick(&mut self, now: Instant) -> Vec<u8> {
        if self.done || !self.approved || self.next.is_some_and(|deadline| now < deadline) {
            return Vec::new();
        }
        if self.text.is_empty() {
            self.done = true;
            return Vec::new();
        }
        self.checkpoint = Some((self.cursor, self.opened, self.done, self.next));
        let mut bytes = Vec::new();
        if self.settings.bracketed && !self.opened {
            bytes.extend_from_slice(b"\x1b[200~");
            self.opened = true;
        }
        let remaining = &self.text[self.cursor..];
        let line_end = if self.settings.line_delay_ms > 0 {
            remaining
                .find(['\r', '\n'])
                .map(|i| {
                    let mut end = i + 1;
                    if remaining.as_bytes()[i] == b'\r'
                        && remaining.as_bytes().get(end) == Some(&b'\n')
                    {
                        end += 1;
                    }
                    end
                })
                .unwrap_or(remaining.len())
        } else {
            remaining.len()
        };
        let mut count = line_end.min(CHUNK);
        while !remaining.is_char_boundary(count) {
            count -= 1;
        }
        bytes.extend_from_slice(&remaining.as_bytes()[..count]);
        self.cursor += count;
        if self.cursor == self.text.len() {
            if self.opened {
                bytes.extend_from_slice(b"\x1b[201~");
                self.opened = false;
            }
            self.done = true;
        } else if count == line_end && self.settings.line_delay_ms > 0 {
            self.next = Some(now + Duration::from_millis(self.settings.line_delay_ms as u64));
        } else {
            self.next = Some(now);
        }
        bytes
    }
    pub fn retry_last_chunk(&mut self) {
        if let Some((cursor, opened, done, next)) = self.checkpoint.take() {
            self.cursor = cursor;
            self.opened = opened;
            self.done = done;
            self.next = next;
        }
    }
    pub fn cancel(&mut self) -> Vec<u8> {
        self.checkpoint = None;
        self.done = true;
        self.text.clear();
        let close = self.opened;
        self.opened = false;
        if close {
            b"\x1b[201~".to_vec()
        } else {
            Vec::new()
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    Close,
    Confirm,
    Cancel,
}
#[derive(Default)]
pub struct CloseController {
    pending: bool,
}
impl CloseController {
    pub fn begin(&mut self, active: bool, confirm: bool) -> CloseDecision {
        self.pending = active && confirm;
        if self.pending {
            CloseDecision::Confirm
        } else {
            CloseDecision::Close
        }
    }
    pub fn resolve(&mut self, approve: bool) -> CloseDecision {
        if !self.pending {
            return CloseDecision::Cancel;
        }
        self.pending = false;
        if approve {
            CloseDecision::Close
        } else {
            CloseDecision::Cancel
        }
    }
}

/// Existing adapters expose strings; only their explicit admission/backpressure errors retry.
pub fn retryable_input_error(error: &str) -> bool {
    error.contains("queue full")
        || error.contains("queue is full")
        || error == "SSH connection is busy or disconnected. Wait, then retry."
}
