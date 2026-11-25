//! Clipboard reads carry their initiating endpoint generations, independently of later GUI focus.
use super::*;
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
const MAX_TEXT: usize = 4 * 1024 * 1024;
pub(super) struct Pending {
    pub source: u64,
    pub targets: Vec<u64>,
    rx: Receiver<Result<String, String>>,
}
struct Finished(Arc<AtomicBool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn read_os_text() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        match read_wayland_text() {
            Ok(text) => return Ok(text),
            Err(error) if error == "Clipboard text exceeds 4 MiB." => return Err(error),
            Err(_) => {}
        }
    }
    let mut clipboard = arboard::Clipboard::new().map_err(|_| {
        "The native clipboard provider is unavailable. Use keyboard paste.".to_owned()
    })?;
    let text=clipboard.get_text().map_err(|_|"The clipboard is empty, busy, or contains no supported text. Use keyboard paste or retry.".to_owned())?;
    if text.len() > MAX_TEXT {
        return Err("Clipboard text exceeds 4 MiB.".into());
    }
    Ok(text)
}
#[cfg(target_os = "linux")]
fn read_wayland_text() -> Result<String, String> {
    let mut child = Command::new("wl-paste")
        .arg("--no-newline")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Wayland clipboard helper unavailable.".to_owned())?;
    let output = child.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = output
            .take((MAX_TEXT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = send.try_send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = None;
    loop {
        if bytes.is_none() {
            if let Ok(result) = receive.try_recv() {
                match result {
                    Ok(value) if value.len() <= MAX_TEXT => bytes = Some(value),
                    Ok(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("Clipboard text exceeds 4 MiB.".into());
                    }
                    Err(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("Wayland clipboard read failed.".into());
                    }
                }
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err("Wayland clipboard read failed.".into());
                }
                if let Some(bytes) = bytes.take() {
                    return String::from_utf8(bytes)
                        .map_err(|_| "Clipboard text is not valid UTF-8.".into());
                }
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Wayland clipboard read failed.".into());
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Wayland clipboard read timed out.".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
impl Workspace {
    pub(super) fn request_clipboard(&mut self, source: u64, ctx: &Context) {
        self.request_clipboard_with(source, ctx, read_os_text);
    }
    pub(super) fn request_clipboard_with(
        &mut self,
        source: u64,
        ctx: &Context,
        read: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) {
        if self.clipboard_read.is_some()
            || self
                .clipboard_busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            self.message("A clipboard read is still running. Wait or use keyboard paste.");
            return;
        }
        let targets = self.paste_targets(source);
        if !self.tabs.iter().any(|t| t.id == source)
            || targets.is_empty()
            || targets.iter().any(|id| {
                !self
                    .tabs
                    .iter()
                    .any(|t| t.id == *id && t.ready_for_input() && !t.input_busy())
            })
        {
            self.clipboard_busy.store(false, Ordering::Release);
            self.message("Clipboard paste was not requested: its original targets are not ready.");
            return;
        }
        let (send, rx) = mpsc::sync_channel(1);
        let busy = self.clipboard_busy.clone();
        let repaint = ctx.clone();
        match std::thread::Builder::new()
            .name("termcraft-clipboard".into())
            .spawn(move || {
                let _finished = Finished(busy);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
                    .unwrap_or_else(|_| Err("The clipboard provider stopped unexpectedly.".into()))
                    .and_then(|text| {
                        if text.len() > MAX_TEXT {
                            Err("Clipboard text exceeds 4 MiB.".into())
                        } else {
                            Ok(text.replace("\r\n", "\n"))
                        }
                    });
                let _ = send.try_send(result);
                repaint.request_repaint();
            }) {
            Ok(_) => {
                self.clipboard_read = Some(Pending {
                    source,
                    targets,
                    rx,
                })
            }
            Err(_) => {
                self.clipboard_busy.store(false, Ordering::Release);
                self.message("Could not start the clipboard reader. Use keyboard paste.");
            }
        }
    }
    pub(super) fn poll_clipboard(&mut self) {
        let result = match self
            .clipboard_read
            .as_ref()
            .map(|pending| pending.rx.try_recv())
        {
            Some(Ok(result)) => result,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                Err("The clipboard reader has closed.".into())
            }
            _ => return,
        };
        let pending = self.clipboard_read.take().unwrap();
        if !self.tabs.iter().any(|t| t.id == pending.source) {
            self.message("Clipboard result discarded: its original terminal has closed.");
            return;
        }
        match result {
            Ok(text) => self.queue_paste_to(pending.targets, text, Vec::new()),
            Err(error) => self.message(error.chars().take(512).collect::<String>()),
        }
    }
}
