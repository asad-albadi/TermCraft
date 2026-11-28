use super::*;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
pub(super) struct Pending {
    result: mpsc::Receiver<Result<harbor_workspace::http_server::Server, String>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Workspace {
    pub(super) fn start_http_share(&mut self) {
        if self.http.is_some() || self.http_start.is_some() {
            return;
        }
        if self.http_auth_enabled && self.http_auth_token.is_empty() {
            self.message("Enter an access token before starting authenticated sharing");
            return;
        }
        let mut config = harbor_workspace::http_server::HttpShareConfig::new(
            PathBuf::from(&self.http_root),
            self.http_bind.clone(),
        );
        config.uploads = self.http_uploads;
        config.timeout = Duration::from_secs(self.http_request_timeout as u64);
        if self.http_auth_enabled {
            config.auth = Some(harbor_workspace::credentials::Secret::new(
                self.http_auth_token.to_string(),
            ));
        }
        use zeroize::Zeroize;
        self.http_auth_token.zeroize();
        let (tx, result) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        match std::thread::Builder::new()
            .name("http-share-start".into())
            .spawn(move || {
                if stop.load(Ordering::Acquire) {
                    return;
                }
                let server = harbor_workspace::http_server::Server::start_with_config(config);
                if !stop.load(Ordering::Acquire) {
                    let _ = tx.send(server);
                }
            }) {
            Ok(_) => {
                self.http_start = Some(Pending { result, cancel });
                self.message("Starting HTTP sharing…");
            }
            Err(error) => self.message(format!("Could not start HTTP sharing: {error}")),
        }
    }
    pub(super) fn poll_http_start(&mut self) {
        let result = match &self.http_start {
            Some(pending) => match pending.result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("HTTP startup worker stopped".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            },
            None => None,
        };
        if let Some(result) = result {
            self.http_start = None;
            match result {
                Ok(server) => {
                    self.http = Some(server);
                    self.http_started = Some(Instant::now());
                    self.http_log.clear();
                    self.message("HTTP sharing started");
                }
                Err(error) => self.message(format!("Could not start HTTP sharing: {error}")),
            }
        }
    }
    pub(super) fn http_share_controls(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.http_uploads, "Allow uploads (new files only)");
        ui.checkbox(&mut self.http_auth_enabled, "Require an access token");
        if self.http_auth_enabled {
            ui.label("Bearer access token (kept only for this server run)");
            super::remote_files::auth_field_scoped(
                ui,
                "http-share-token",
                0,
                0,
                0,
                &harbor_workspace::ssh::Prompt {
                    label: "Access token".into(),
                    echo: false,
                },
                &mut self.http_auth_token,
            );
            ui.small("Clients send Authorization: Bearer TOKEN. HTTP is not encrypted; use a trusted connection or a TLS reverse proxy.");
        }
        ui.collapsing("Advanced sharing options", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Request deadline");
            ui.add(
                DragValue::new(&mut self.http_request_timeout)
                    .range(1..=3600)
                    .suffix(" seconds"),
            );
        });
        ui.small("GET/HEAD support byte ranges. PUT uploads are limited to 64 MiB and never replace existing files. Uploads require a filesystem with hard-link support; FAT/exFAT uploads are unavailable.");        });
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    #[test]
    fn http_ui_background_start_applies_private_auth_and_upload_policy() {
        let (_ctx, _workspace_root, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("hello.txt"), b"hello").unwrap();
        w.http_root = root.path().display().to_string();
        w.http_bind = "127.0.0.1:0".into();
        w.http_auth_enabled = true;
        w.http_uploads = true;
        w.http_auth_token.push_str("fixture-token");
        w.start_http_share();
        assert!(w.http.is_none());
        assert!(w.http_start.is_some());
        assert!(
            w.http_auth_token.is_empty(),
            "Token retained in UI after dispatch"
        );
        let end = Instant::now() + Duration::from_secs(3);
        while w.http.is_none() {
            w.poll_http_start();
            assert!(Instant::now() < end, "{}", w.status);
            std::thread::sleep(Duration::from_millis(5));
        }
        let address = w.http.as_ref().unwrap().address.clone();
        let request = |bytes: &[u8]| {
            let mut stream = std::net::TcpStream::connect(&address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream.write_all(bytes).unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        assert!(
            request(b"GET /hello.txt HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .starts_with("HTTP/1.1 401")
        );
        let reply=request(b"GET /hello.txt HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer fixture-token\r\n\r\n");
        assert!(reply.starts_with("HTTP/1.1 200"));
        assert!(reply.ends_with("hello"));
        let reply=request(b"PUT /new.txt HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer fixture-token\r\nContent-Length: 3\r\n\r\nnew");
        assert!(reply.starts_with("HTTP/1.1 201"));
        assert_eq!(fs::read(root.path().join("new.txt")).unwrap(), b"new");
        assert!(
            !serde_json::to_string(&w.store)
                .unwrap()
                .contains("fixture-token")
        );
        w.http = None;
    }
}
