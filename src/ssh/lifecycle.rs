//! One connection attempt owns its prompts; cancellation and answer submission share a gate.
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
static NEXT_PROMPT: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy)]
enum Prompt {
    None,
    Waiting(u64),
    Submitted(u64),
}
struct State {
    cancelled: bool,
    prompt: Prompt,
}
pub(super) struct ConnectionLifecycle {
    state: Mutex<State>,
    prompt_waiting: tokio::sync::watch::Sender<bool>,
}
impl ConnectionLifecycle {
    pub fn new() -> Self {
        Self {
            prompt_waiting: tokio::sync::watch::channel(false).0,
            state: Mutex::new(State {
                cancelled: false,
                prompt: Prompt::None,
            }),
        }
    }
    pub fn prompt_state(&self) -> tokio::sync::watch::Receiver<bool> {
        self.prompt_waiting.subscribe()
    }
    pub fn ensure_active(&self) -> Result<(), String> {
        if self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .cancelled
        {
            Err("Authentication cancelled".into())
        } else {
            Ok(())
        }
    }
    pub fn cancelled(&self) -> bool {
        self.ensure_active().is_err()
    }
    pub fn begin_prompt(&self) -> Result<u64, String> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.cancelled {
            return Err("Authentication cancelled".into());
        }
        #[allow(deprecated)]
        let id = NEXT_PROMPT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| "Authentication prompt identities exhausted")?;
        state.prompt = Prompt::Waiting(id);
        self.prompt_waiting.send_replace(true);
        Ok(id)
    }
    pub fn submit(&self, id: u64, send: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.cancelled || !matches!(state.prompt, Prompt::Waiting(expected) if expected == id) {
            return Err(
                "This authentication prompt is cancelled, stale or already answered".into(),
            );
        }
        send()?;
        state.prompt = Prompt::Submitted(id);
        Ok(())
    }
    pub fn consume(&self, id: u64) -> Result<(), String> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.cancelled || !matches!(state.prompt, Prompt::Submitted(expected) if expected==id) {
            return Err("Authentication prompt is no longer active".into());
        }
        state.prompt = Prompt::None;
        self.prompt_waiting.send_replace(false);
        Ok(())
    }
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.cancelled = true;
        state.prompt = Prompt::None;
        self.prompt_waiting.send_replace(false);
    }
}
