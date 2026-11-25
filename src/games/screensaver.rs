//! Local idle policy. It neither owns nor changes terminal sessions or input queues.
use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub struct ScreensaverState {
    timeout: Option<Duration>,
    idle: Duration,
    active: bool,
}

impl ScreensaverState {
    /// None (the default) and zero disable the saver. Changes reset idle time.
    pub fn configure(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout.filter(|timeout| !timeout.is_zero());
        self.idle = Duration::ZERO;
        self.active = false;
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    pub fn tick(&mut self, dt: Duration) {
        if let Some(timeout) = self.timeout {
            self.idle = self.idle.saturating_add(dt);
            self.active = self.idle >= timeout;
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Returns whether this input dismissed an active saver. The caller routes
    /// that frame exclusively to dismissal, keeping hidden controls and terminal
    /// broadcast from receiving it. Previously queued terminal input is untouched.
    pub fn on_input(&mut self) -> bool {
        let dismissed = self.active;
        self.active = false;
        self.idle = Duration::ZERO;
        dismissed
    }
}
