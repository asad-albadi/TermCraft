//! Bounded macro execution tied to one authenticated runtime session.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    time::Duration,
};

const MAX_EVENTS: usize = 4096;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_MILLIS: u64 = 3_600_000;
const MAX_BATCH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub tab: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Text(String),
    Key {
        code: u32,
        pressed: bool,
    },
    /// Coordinates span the current remote surface from 0 through 65535.
    Pointer {
        x: u16,
        y: u16,
        buttons: u8,
    },
    ReleaseAll,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Input(Action),
    Wait {
        millis: u64,
    },
    WaitPattern {
        pattern: String,
        timeout_millis: u64,
    },
    WaitFocus {
        timeout_millis: u64,
    },
    Call {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub version: u32,
    pub events: Vec<Event>,
}

#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub target: Target,
    pub now: Duration,
    pub authenticated: bool,
    pub focused: bool,
    /// New output from this target since the last poll, not its screen history.
    pub output: &'a str,
}

pub struct Recorder {
    target: Target,
    last: Duration,
    script: Script,
    bytes: usize,
    stopped: bool,
}
impl Recorder {
    pub fn new(target: Target, now: Duration) -> Self {
        Self {
            target,
            last: now,
            script: Script {
                version: 1,
                events: Vec::new(),
            },
            bytes: 0,
            stopped: false,
        }
    }
    pub fn record(&mut self, context: Context<'_>, action: Action) -> Result<bool, String> {
        if self.stopped {
            return Ok(false);
        }
        if context.target != self.target || context.now < self.last {
            self.stopped = true;
            return Err("Macro recording stopped: target or clock changed".into());
        }
        if !context.authenticated {
            self.stopped = true;
            return Ok(false);
        }
        let millis = u64::try_from((context.now - self.last).as_millis()).unwrap_or(u64::MAX);
        let mut bytes = self.bytes;
        validate(&Event::Input(action.clone()), &mut bytes)?;
        if millis > MAX_MILLIS
            || self.script.events.len() + 1 + usize::from(millis != 0) > MAX_EVENTS
        {
            self.stopped = true;
            return Err("Macro recording limit reached".into());
        }
        if millis != 0 {
            self.script.events.push(Event::Wait { millis });
        }
        self.script.events.push(Event::Input(action));
        self.last = context.now;
        self.bytes = bytes;
        Ok(true)
    }
    pub fn finish(self) -> Script {
        self.script
    }
}

pub struct Player {
    target: Target,
    events: VecDeque<Event>,
    due: Duration,
    last: Duration,
    pattern: String,
    held_keys: BTreeSet<u32>,
    pointer_held: bool,
    cleanup: Vec<Action>,
}
impl Player {
    pub fn new(
        target: Target,
        script: Script,
        library: &[(String, Script)],
        now: Duration,
    ) -> Result<Self, String> {
        let mut names = BTreeSet::new();
        for (name, _) in library {
            if name.is_empty() || !names.insert(name) {
                return Err("Macro library has empty or ambiguous names".into());
            }
        }
        let mut events = VecDeque::new();
        expand(
            &script,
            library,
            &mut Vec::new(),
            &mut events,
            &mut 0,
            &mut 0,
        )?;
        Ok(Self {
            target,
            events,
            due: now,
            last: now,
            pattern: String::new(),
            held_keys: BTreeSet::new(),
            pointer_held: false,
            cleanup: Vec::new(),
        })
    }
    pub fn events_match(&self, predicate: impl FnMut(&Event) -> bool) -> bool {
        self.events.iter().all(predicate)
    }
    pub fn finished(&self) -> bool {
        self.events.is_empty()
    }
    /// The transport accepted input. Output from before this boundary cannot
    /// satisfy the next pattern wait, and its timeout starts no earlier than delivery.
    pub fn input_submitted(&mut self, now: Duration) {
        self.pattern.clear();
        if matches!(self.events.front(), Some(Event::WaitPattern { .. })) {
            self.due = self.due.max(now);
        }
    }

    pub fn take_cleanup(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.cleanup)
    }
    pub fn cancel(&mut self) -> Vec<Action> {
        self.events.clear();
        self.pattern.clear();
        self.held_keys.clear();
        self.pointer_held = false;
        self.cleanup.clear();
        vec![Action::ReleaseAll]
    }
    fn fail(&mut self, message: &str) -> Result<Vec<Action>, String> {
        self.cleanup = self.cancel();
        Err(message.into())
    }
    pub fn tick(&mut self, context: Context<'_>) -> Result<Vec<Action>, String> {
        if self.finished() {
            return Ok(Vec::new());
        }
        if context.target != self.target || !context.authenticated {
            return self.fail("Macro canceled: target changed or authentication is pending");
        }
        if context.now < self.last {
            return self.fail("Macro canceled: clock moved backwards");
        }
        if self.pattern.len().saturating_add(context.output.len()) > MAX_BYTES {
            return self.fail("Macro canceled: buffered output exceeds 1 MiB");
        }
        self.pattern.push_str(context.output);
        self.last = context.now;
        let mut output = Vec::new();
        let mut may_read_output = true;
        let mut processed = 0;
        while processed < MAX_BATCH {
            let Some(event) = self.events.front().cloned() else {
                break;
            };
            if context.now < self.due {
                break;
            }
            match event {
                Event::Input(action) => {
                    match &action {
                        Action::Key {
                            code,
                            pressed: true,
                        } => {
                            self.held_keys.insert(*code);
                        }
                        Action::Key {
                            code,
                            pressed: false,
                        } => {
                            self.held_keys.remove(code);
                        }
                        Action::Pointer { buttons, .. } => self.pointer_held = *buttons != 0,
                        Action::ReleaseAll => {
                            self.held_keys.clear();
                            self.pointer_held = false;
                        }
                        _ => {}
                    }
                    output.push(action);
                    self.pattern.clear();
                    may_read_output = false;
                }
                Event::Wait { millis } => {
                    self.due = self.due.saturating_add(Duration::from_millis(millis));
                }
                Event::WaitPattern {
                    pattern,
                    timeout_millis,
                } => {
                    if !may_read_output {
                        self.due = context.now;
                        break;
                    }
                    if let Some(start) = self.pattern.find(&pattern) {
                        self.pattern.drain(..start + pattern.len());
                        self.due = context.now;
                    } else if context.now
                        >= self
                            .due
                            .saturating_add(Duration::from_millis(timeout_millis))
                    {
                        return self.fail("Macro canceled: output-pattern wait timed out");
                    } else {
                        let mut boundary = self
                            .pattern
                            .len()
                            .saturating_sub(pattern.len().saturating_sub(1));
                        while !self.pattern.is_char_boundary(boundary) {
                            boundary += 1;
                        }
                        self.pattern.drain(..boundary);
                        break;
                    }
                }
                Event::WaitFocus { timeout_millis } => {
                    if context.focused {
                        self.due = context.now;
                    } else if context.now
                        >= self
                            .due
                            .saturating_add(Duration::from_millis(timeout_millis))
                    {
                        return self.fail("Macro canceled: focus wait timed out");
                    } else {
                        break;
                    }
                }
                Event::Call { .. } => unreachable!("calls are expanded before execution"),
            }
            self.events.pop_front();
            processed += 1;
            if self.events.is_empty() && (!self.held_keys.is_empty() || self.pointer_held) {
                self.events.push_back(Event::Input(Action::ReleaseAll));
            }
        }
        Ok(output)
    }
}

fn validate(event: &Event, bytes: &mut usize) -> Result<(), String> {
    match event {
        Event::Input(Action::Text(text)) => *bytes = bytes.saturating_add(text.len()),
        Event::Input(Action::Key { code, .. }) if *code > 0x1ff || *code & 0xff == 0 => {
            return Err("Macro key has an invalid scan code".into());
        }
        Event::Input(Action::Pointer { buttons, .. }) if *buttons & !7 != 0 => {
            return Err("Macro pointer has unsupported buttons".into());
        }
        Event::Wait { millis } if *millis > MAX_MILLIS => {
            return Err("Macro delay exceeds one hour".into());
        }
        Event::WaitFocus { timeout_millis } | Event::WaitPattern { timeout_millis, .. }
            if *timeout_millis == 0 || *timeout_millis > MAX_MILLIS =>
        {
            return Err("Macro wait requires a timeout from 1 ms to one hour".into());
        }
        _ => {}
    }
    if let Event::WaitPattern { pattern, .. } = event {
        if pattern.is_empty() || pattern.len() > 4096 {
            return Err("Macro output pattern must contain 1–4096 bytes".into());
        }
        *bytes = bytes.saturating_add(pattern.len());
    }
    if *bytes > MAX_BYTES {
        return Err("Macro text exceeds 1 MiB".into());
    }
    Ok(())
}

fn expand(
    script: &Script,
    library: &[(String, Script)],
    stack: &mut Vec<String>,
    out: &mut VecDeque<Event>,
    bytes: &mut usize,
    visited: &mut usize,
) -> Result<(), String> {
    if script.version != 1 {
        return Err("Unsupported macro script version".into());
    }
    if stack.len() > 8 || script.events.len() > MAX_EVENTS {
        return Err("Macro nesting or event limit exceeded".into());
    }
    for event in &script.events {
        *visited += 1;
        if *visited > MAX_EVENTS {
            return Err("Expanded macro exceeds 4096 steps".into());
        }
        validate(event, bytes)?;
        if let Event::Call { name } = event {
            if stack.contains(name) {
                return Err("Recursive macro call rejected".into());
            }
            let child = library
                .iter()
                .find(|(n, _)| n == name)
                .ok_or("Named macro was not found")?;
            stack.push(name.clone());
            expand(&child.1, library, stack, out, bytes, visited)?;
            stack.pop();
        } else {
            if out.len() >= MAX_EVENTS {
                return Err("Expanded macro exceeds 4096 events".into());
            }
            out.push_back(event.clone());
        }
    }
    Ok(())
}
