//! Private Chromium workspace sessions.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod engine;
#[cfg(any(target_os = "macos", test))]
pub(crate) mod native_macos;
#[cfg(any(windows, test))]
pub(crate) mod native_windows;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod pipe;
#[cfg(windows)]
#[path = "windows_pipe.rs"]
mod pipe;

/// Enter the private Windows Chromium launch helper when requested by its controller.
#[doc(hidden)]
#[cfg(windows)]
pub fn windows_bootstrap() -> Option<i32> {
    pipe::bootstrap::run_if_requested()
}

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender},
};
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use std::thread;

use serde::{Deserialize, Serialize};

/// Explicit engine choice. Absent values preserve existing system-managed profiles.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Runtime {
    #[default]
    SystemChromium,
    BundledChromium,
}

impl Runtime {
    fn is_system(&self) -> bool {
        *self == Self::SystemChromium
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Options {
    pub embedded: bool,
    #[serde(skip_serializing_if = "Runtime::is_system")]
    pub runtime: Runtime,
    #[serde(
        deserialize_with = "deserialize_proxy",
        serialize_with = "serialize_proxy",
        skip_serializing_if = "String::is_empty"
    )]
    pub proxy: String,
}

impl Options {
    pub fn is_legacy(&self) -> bool {
        !self.embedded && self.proxy.is_empty() && self.runtime == Runtime::SystemChromium
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Options")
            .field("embedded", &self.embedded)
            .field("runtime", &self.runtime)
            .field("proxy_configured", &!self.proxy.is_empty())
            .finish()
    }
}

const INVALID_PROXY: &str = "<invalid endpoint>";

fn deserialize_proxy<'de, D: serde::Deserializer<'de>>(input: D) -> Result<String, D::Error> {
    String::deserialize(input)
        .map(|proxy| validated_proxy(&proxy).unwrap_or_else(|_| INVALID_PROXY.into()))
}

fn serialize_proxy<S: serde::Serializer>(proxy: &str, output: S) -> Result<S::Ok, S::Error> {
    output.serialize_str(&validated_proxy(proxy).unwrap_or_else(|_| INVALID_PROXY.into()))
}

pub struct Config {
    pub generation: u64,
    pub address: String,
    pub proxy: String,
    pub size: [u32; 2],
}

pub struct Credentials {
    pub username: zeroize::Zeroizing<String>,
    pub password: zeroize::Zeroizing<String>,
}

pub enum Command {
    Navigate(String),
    History(i32),
    Reload,
    Stop,
    Resize {
        size: [u32; 2],
        revision: u64,
    },
    Pointer {
        kind: &'static str,
        button: &'static str,
        x: f64,
        y: f64,
    },
    CancelPointer,
    Wheel {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    Key {
        down: bool,
        key: String,
        code: String,
        virtual_key: u32,
        modifiers: u8,
        repeat: bool,
    },
    Text(String),
    Composition {
        text: String,
        selection: [usize; 2],
    },
    CopySelection,
    Auth {
        challenge: u64,
        credentials: Option<Credentials>,
    },
}

#[derive(Clone)]
pub struct Challenge {
    pub id: u64,
    pub proxy: bool,
    pub origin: String,
}

#[derive(Clone, Default)]
pub struct State {
    pub title: String,
    pub address: String,
    pub loading: bool,
    pub closed: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub challenge: Option<Challenge>,
}

pub struct Frame {
    pub generation: u64,
    pub viewport_revision: u64,
    pub size: [u32; 2],
    pub rgba: Vec<u8>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    frame: Mutex<Option<Frame>>,
    copy: Mutex<Option<String>>,
    cancelled: AtomicBool,
}

pub struct Session {
    pub generation: u64,
    tx: SyncSender<Command>,
    shared: Arc<Shared>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    worker: Option<thread::JoinHandle<()>>,
}

impl Session {
    pub fn start(config: Config) -> Result<Self, String> {
        Self::start_with_runtime(config, Runtime::SystemChromium)
    }

    pub fn start_with_runtime(mut config: Config, runtime: Runtime) -> Result<Self, String> {
        config.address = validated_address(&config.address, true)?;
        config.proxy = validated_proxy(&config.proxy)?;
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        return Err("Embedded Browser is unavailable on this platform.".into());
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        {
            config.size = bounded_size(config.size);
            let generation = config.generation;
            let (tx, rx) = mpsc::sync_channel(128);
            let shared = Arc::new(Shared::default());
            {
                let mut state = shared.state.lock().unwrap();
                state.address = config.address.clone();
                state.loading = true;
            }
            let worker = shared.clone();
            let join = thread::Builder::new()
                .name("embedded-browser".into())
                .spawn(move || {
                    let result = engine::run(config, runtime, &worker, rx);
                    let mut state = worker.state.lock().unwrap();
                    state.loading = false;
                    state.closed = true;
                    state.challenge = None;
                    if !worker.cancelled.load(Ordering::Acquire) {
                        state.error = result.err().or(Some("The browser engine exited.".into()));
                    }
                })
                .map_err(|_| "Could not start browser worker.".to_owned())?;
            Ok(Self {
                generation,
                tx,
                shared,
                worker: Some(join),
            })
        }
    }

    pub fn state(&self) -> State {
        self.shared.state.lock().unwrap().clone()
    }

    pub fn take_frame(&self) -> Option<Frame> {
        self.shared.frame.lock().unwrap().take()
    }

    pub fn take_copy(&self) -> Option<String> {
        self.shared.copy.lock().unwrap().take()
    }

    pub fn send(&self, command: Command) -> Result<(), &'static str> {
        let valid = match &command {
            Command::Navigate(text) => text.len() <= 8192,
            Command::Text(text) | Command::Composition { text, .. } => text.len() <= 65536,
            Command::Key { key, code, .. } => key.len() <= 32 && code.len() <= 32,
            Command::Auth {
                credentials: Some(credentials),
                ..
            } => credentials.username.len() <= 1024 && credentials.password.len() <= 4096,
            _ => true,
        };
        if !valid {
            return Err("Browser input exceeds its size limit.");
        }
        self.tx.try_send(command).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => "Browser is busy. Try again.",
            mpsc::TrySendError::Disconnected(_) => "Browser has closed.",
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if let Some(worker) = self.worker.take() {
            #[cfg(not(windows))]
            let _ = worker.join();
            #[cfg(windows)]
            {
                // Terminate and reap the owned job off the UI thread.
                let mut pending = WINDOWS_CLEANUP.lock().unwrap();
                pending.retain(|worker| !worker.is_finished());
                pending.push(worker);
            }
        }
    }
}

#[cfg(windows)]
static WINDOWS_CLEANUP: Mutex<Vec<thread::JoinHandle<()>>> = Mutex::new(Vec::new());

/// Join Windows browser workers after the workspace drops its open sessions.
#[cfg(windows)]
pub fn wait_for_shutdown() {
    let workers = std::mem::take(&mut *WINDOWS_CLEANUP.lock().unwrap());
    for worker in workers {
        let _ = worker.join();
    }
}

pub fn validated_address(value: &str, allow_blank: bool) -> Result<String, String> {
    let value = value.trim();
    if allow_blank && matches!(value, "" | "about:blank") {
        return Ok("about:blank".into());
    }
    if value.len() <= 8192
        && !value.chars().any(char::is_control)
        && let Ok(url) = url::Url::parse(value)
        && matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
    {
        return Ok(url.into());
    }
    Err("Enter an HTTP or HTTPS address without embedded credentials.".into())
}

pub fn validated_proxy(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    let url = url::Url::parse(&validated_address(value, false)?)
        .map_err(|_| "Invalid proxy endpoint.".to_owned())?;
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err("Enter an HTTP or HTTPS proxy endpoint without a path or credentials.".into());
    }
    Ok(url.origin().ascii_serialization())
}

pub fn bounded_size([width, height]: [u32; 2]) -> [u32; 2] {
    [width.clamp(1, 1920), height.clamp(1, 1080)]
}

#[cfg(test)]
mod runtime_selection_tests {
    use super::*;
    #[test]
    fn old_profiles_preserve_installed_provider_and_config_api() {
        let options: Options = serde_json::from_str(r#"{"embedded":true}"#).unwrap();
        assert_eq!(options.runtime, Runtime::SystemChromium);
        assert_eq!(
            serde_json::to_value(options).unwrap(),
            serde_json::json!({"embedded": true})
        );
        let options: Options =
            serde_json::from_str(r#"{"embedded":true,"runtime":"BundledChromium"}"#).unwrap();
        assert_eq!(options.runtime, Runtime::BundledChromium);
        assert_eq!(
            serde_json::to_value(options).unwrap()["runtime"],
            "BundledChromium"
        );
    }
}

#[cfg(all(
    test,
    any(
        target_os = "linux",
        all(
            target_os = "macos",
            target_arch = "aarch64",
            feature = "macos-chromium-qualification"
        )
    )
))]
mod bundled_cancellation_qualification {
    use super::*;
    #[test]
    #[ignore = "requires relocated full pinned browser bundle beside test executable"]
    fn close_during_whole_resource_verification_is_chunk_cancellable() {
        crate::runtime::VERIFIED_CHUNKS.store(0, Ordering::Release);
        let session = Session::start_with_runtime(
            Config {
                generation: 42,
                address: "http://127.0.0.1/".into(),
                proxy: String::new(),
                size: [640, 480],
            },
            Runtime::BundledChromium,
        )
        .unwrap();
        let shared = session.shared.clone();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while crate::runtime::VERIFIED_CHUNKS.load(Ordering::Acquire) < 4 {
            assert!(
                std::time::Instant::now() < deadline,
                "verification did not begin"
            );
            std::thread::yield_now();
        }
        let started = std::time::Instant::now();
        drop(session);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "UI close waited {:?} for verification",
            started.elapsed()
        );
        assert!(shared.state.lock().unwrap().closed);
    }
}
