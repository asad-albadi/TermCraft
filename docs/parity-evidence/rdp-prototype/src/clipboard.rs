use crate::{
    clipapi::{backend::ClipboardMessage, backend::CliprdrBackendFactory},
    clipboard_backend as text, server,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
pub struct Factory {
    pub sender: Option<tokio::sync::mpsc::UnboundedSender<server::ServerEvent>>,
    pub path: PathBuf,
}
impl server::ServerEventSender for Factory {
    fn set_sender(&mut self, sender: tokio::sync::mpsc::UnboundedSender<server::ServerEvent>) {
        self.sender = Some(sender);
    }
}
impl server::CliprdrServerFactory for Factory {}
impl CliprdrBackendFactory for Factory {
    fn build_cliprdr_backend(&self) -> Box<dyn crate::clipapi::backend::CliprdrBackend> {
        let sender = self.sender.clone().unwrap();
        let path = self.path.clone();
        let local = Arc::new(Mutex::new(Some(zeroize::Zeroizing::new(
            "Remote clipboard 🐧".into(),
        ))));
        Box::new(text::Backend {
            to_remote: true,
            from_remote: true,
            local,
            emit: Arc::new(move |action| {
                let record = |value: &str| {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                    {
                        let _ = writeln!(f, "{value}");
                    }
                };
                let message = match action {
                    text::Action::Ready => {
                        record("clipboard:ready");
                        ClipboardMessage::SendInitiateCopy(text::formats().to_vec())
                    }
                    text::Action::Text(value) => {
                        record(&format!("clipboard:{value}"));
                        return;
                    }
                    text::Action::Offer => {
                        ClipboardMessage::SendInitiateCopy(text::formats().to_vec())
                    }
                    text::Action::Request => ClipboardMessage::SendInitiatePaste(
                        crate::clipapi::pdu::ClipboardFormatId::CF_UNICODETEXT,
                    ),
                    text::Action::Data(data) => ClipboardMessage::SendFormatData(data),
                };
                let _ = sender.send(server::ServerEvent::Clipboard(message));
            }),
        })
    }
}
