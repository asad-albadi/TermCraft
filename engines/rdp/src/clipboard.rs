//! Text-only clipboard backend. No filesystem or OS clipboard access in the engine.
use crate::clipapi::{backend::CliprdrBackend, pdu::*};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;
pub const MAX_TEXT: usize = 8192;
pub enum Action {
    Offer,
    Request,
    Data(OwnedFormatDataResponse),
    Text(String),
    Ready,
}
pub struct Backend {
    pub to_remote: bool,
    pub from_remote: bool,
    pub local: Arc<Mutex<Option<Zeroizing<String>>>>,
    pub emit: Arc<dyn Fn(Action) + Send + Sync>,
}
impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TextClipboardBackend")
    }
}
impl crate::clipcore::AsAny for Backend {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
impl CliprdrBackend for Backend {
    fn temporary_directory(&self) -> &str {
        ""
    }
    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
    }
    fn on_ready(&mut self) {
        (self.emit)(Action::Ready);
    }
    fn on_request_format_list(&mut self) {
        (self.emit)(Action::Offer);
    }
    fn on_process_negotiated_capabilities(&mut self, _: ClipboardGeneralCapabilityFlags) {}
    fn on_remote_copy(&mut self, formats: &[ClipboardFormat]) {
        if self.from_remote
            && formats
                .iter()
                .any(|f| f.id == ClipboardFormatId::CF_UNICODETEXT)
        {
            (self.emit)(Action::Request);
        }
    }
    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let text = self.local.lock().unwrap();
        let data = if self.to_remote && request.format == ClipboardFormatId::CF_UNICODETEXT {
            match text.as_ref() {
                Some(text) => FormatDataResponse::new_unicode_string(text),
                None => FormatDataResponse::new_error(),
            }
        } else {
            FormatDataResponse::new_error()
        };
        (self.emit)(Action::Data(data));
    }
    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        let data = response.data();
        if self.from_remote
            && !response.is_error()
            && data.len() >= 2
            && data.len() <= MAX_TEXT * 2 + 2
            && data.len() % 2 == 0
            && data.ends_with(&[0, 0])
        {
            // The library's generic string reader accepts truncated UTF-16.
            // CF_UNICODETEXT must contain complete code units and a terminating NUL.
            let units: Vec<u16> = data[..data.len() - 2]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            if !units.contains(&0) {
                if let Ok(text) = String::from_utf16(&units) {
                    if text.len() <= MAX_TEXT {
                        (self.emit)(Action::Text(text));
                    }
                }
            }
        }
    }
    fn on_file_contents_request(&mut self, _: FileContentsRequest) {}
    fn on_file_contents_response(&mut self, _: FileContentsResponse<'_>) {}
    fn on_lock(&mut self, _: LockDataId) {}
    fn on_unlock(&mut self, _: LockDataId) {}
}
pub fn formats() -> [ClipboardFormat; 1] {
    [ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn backend(to_remote: bool, from_remote: bool) -> (Backend, std::sync::mpsc::Receiver<Action>) {
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        (
            Backend {
                to_remote,
                from_remote,
                local: Arc::new(Mutex::new(None)),
                emit: Arc::new(move |action| {
                    tx.try_send(action)
                        .unwrap_or_else(|_| panic!("unexpected clipboard event overflow"))
                }),
            },
            rx,
        )
    }
    #[test]
    fn empty_local_clipboard_still_initializes_the_channel() {
        let (mut backend, events) = backend(false, true);
        backend.on_request_format_list();
        assert!(matches!(events.try_recv(), Ok(Action::Offer)));
        let mut channel = crate::clipapi::CliprdrClient::new(Box::new(backend));
        assert!(!channel.is_ready());
        let messages: Vec<_> = channel.initiate_copy(&[]).unwrap().into();
        assert_eq!(
            messages.len(),
            3,
            "initial capabilities, directory and empty format list must be sent"
        );
    }
    #[test]
    fn text_channel_enforces_both_directions_and_rejects_invalid_data() {
        for to_remote in [false, true] {
            for from_remote in [false, true] {
                let (mut backend, events) = backend(to_remote, from_remote);
                *backend.local.lock().unwrap() = Some(Zeroizing::new("Local 🐧".into()));
                backend.on_format_data_request(FormatDataRequest {
                    format: ClipboardFormatId::CF_UNICODETEXT,
                });
                let Action::Data(data) = events.try_recv().unwrap() else {
                    panic!("expected protocol response")
                };
                assert_eq!(data.is_error(), !to_remote);
                if to_remote {
                    assert_eq!(data.to_unicode_string().unwrap(), "Local 🐧");
                }
                backend.on_remote_copy(&formats());
                assert_eq!(events.try_recv().is_ok(), from_remote);
                backend
                    .on_format_data_response(FormatDataResponse::new_unicode_string("Remote 🐧"));
                if from_remote {
                    assert!(matches!(events.try_recv(), Ok(Action::Text(t)) if t == "Remote 🐧"));
                } else {
                    assert!(events.try_recv().is_err());
                }
                for response in [
                    FormatDataResponse::new_error(),
                    FormatDataResponse::new_data(vec![0xff]),
                    FormatDataResponse::new_data(vec![0x41, 0]),
                    FormatDataResponse::new_data(vec![0, 0xd8, 0, 0]),
                    FormatDataResponse::new_unicode_string(&"x".repeat(MAX_TEXT + 1)),
                ] {
                    backend.on_format_data_response(response);
                    assert!(
                        events.try_recv().is_err(),
                        "invalid clipboard data was delivered"
                    );
                }
            }
        }
    }
}
