use super::{
    Challenge, Command, Config, Credentials, Frame, Shared, bounded_size, pipe::Pipe,
    validated_address,
};
use base64::Engine as _;
use serde_json::{Value, json};
use std::io::Cursor;
use std::sync::{atomic::Ordering, mpsc::Receiver};
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
const FRAME_INTERVAL: Duration = Duration::from_millis(120);

pub(super) fn run(
    config: Config,
    runtime: super::Runtime,
    shared: &Shared,
    commands: Receiver<Command>,
) -> Result<(), String> {
    if shared.cancelled.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut browser = Browser {
        #[cfg(not(windows))]
        pipe: Pipe::start_with_runtime(&config.proxy, runtime, &shared.cancelled)?,
        #[cfg(windows)]
        pipe: Pipe::start_with_runtime(&config.proxy, &shared.cancelled, runtime)?,
        shared,
        next: 1,
        target: String::new(),
        window: 0,
        session: String::new(),
        generation: config.generation,
        size: config.size,
        viewport_revision: 0,
        main_frame: String::new(),
        history: Vec::new(),
        history_index: 0,
        auth: None,
        capture: None,
        navigation_request: None,
        buttons: 0,
    };
    browser.initialize()?;
    browser.navigate(&config.address)?;
    let mut frame_due = Instant::now();
    let mut metadata_due = frame_due;
    while !shared.cancelled.load(Ordering::Acquire) {
        for command in commands.try_iter().take(24) {
            browser.handle(command)?;
        }
        let now = Instant::now();
        if now >= metadata_due {
            browser.metadata()?;
            metadata_due = now + Duration::from_millis(250);
        }
        if browser
            .capture
            .is_some_and(|capture| now >= capture.deadline)
        {
            browser.capture = None;
        }
        if now >= frame_due && browser.auth.is_none() && browser.capture.is_none() {
            browser.request_frame()?;
            frame_due = now + FRAME_INTERVAL;
        }
        if let Some(event) = browser.pipe.receive(
            Instant::now() + Duration::from_millis(15),
            &shared.cancelled,
        )? {
            browser.event(&event)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Capture {
    id: u64,
    deadline: Instant,
    size: [u32; 2],
    viewport_revision: u64,
}

struct Browser<'a> {
    pipe: Pipe,
    shared: &'a Shared,
    next: u64,
    target: String,
    window: i64,
    session: String,
    generation: u64,
    size: [u32; 2],
    viewport_revision: u64,
    main_frame: String,
    history: Vec<i64>,
    history_index: usize,
    auth: Option<(u64, String)>,
    capture: Option<Capture>,
    navigation_request: Option<String>,
    buttons: u8,
}

impl Browser<'_> {
    fn initialize(&mut self) -> Result<(), String> {
        // Reuse the blank target launched with the private profile and attach before loading any page.
        let targets = self.call(false, "Target.getTargets", json!({}))?;
        self.target = targets["targetInfos"]
            .as_array()
            .and_then(|targets| {
                targets
                    .iter()
                    .find(|entry| entry["type"] == "page")
                    .and_then(|entry| entry["targetId"].as_str())
            })
            .ok_or("Chromium did not create a blank page.")?
            .into();
        let window = self.call(
            false,
            "Browser.getWindowForTarget",
            json!({"targetId":self.target}),
        )?;
        self.window = window["windowId"]
            .as_i64()
            .ok_or("Chromium did not provide an owned window.")?;
        let attach = self.call(
            false,
            "Target.attachToTarget",
            json!({"targetId":self.target,"flatten":true}),
        )?;
        self.session = attach["sessionId"]
            .as_str()
            .ok_or("Could not attach to Chromium page.")?
            .into();
        self.call(
            false,
            "Browser.setDownloadBehavior",
            json!({"behavior":"deny","eventsEnabled":true}),
        )?;
        self.call(false, "Target.setDiscoverTargets", json!({"discover":true}))?;
        // Pause new windows before execution and dismiss them in the attachment handler.
        self.call(
            false,
            "Target.setAutoAttach",
            json!({
                "autoAttach":true,"waitForDebuggerOnStart":true,"flatten":true,
                "filter":[{"type":"page"}]
            }),
        )?;
        self.call(true, "Page.enable", json!({}))?;
        self.call(true, "Network.enable", json!({}))?;
        self.call(
            true,
            "Page.setInterceptFileChooserDialog",
            json!({"enabled":true}),
        )?;
        self.call(
            true,
            "Fetch.enable",
            json!({
                "handleAuthRequests":true,
                "patterns":[{"urlPattern":"*","resourceType":"Document"}]
            }),
        )?;
        let tree = self.call(true, "Page.getFrameTree", json!({}))?;
        self.main_frame = tree["frameTree"]["frame"]["id"]
            .as_str()
            .unwrap_or_default()
            .into();
        self.resize(self.size, 0)
    }

    fn issue(&mut self, target: bool, method: &str, params: Value) -> Result<u64, String> {
        let id = self.next;
        self.next += 1;
        let mut message = json!({"id":id,"method":method,"params":params});
        if target {
            message["sessionId"] = json!(self.session);
        }
        self.pipe.send(&message)?;
        Ok(id)
    }

    fn call(&mut self, target: bool, method: &str, params: Value) -> Result<Value, String> {
        let id = self.issue(target, method, params)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        while let Some(message) = self.pipe.receive(deadline, &self.shared.cancelled)? {
            if message["id"] == id {
                if message.get("error").is_some() {
                    return Err(format!("Chromium could not complete {method}."));
                }
                return Ok(message["result"].clone());
            }
            self.event(&message)?;
        }
        Err(format!("Chromium timed out while completing {method}."))
    }

    fn notice(&self, text: &str) {
        self.shared.state.lock().unwrap().notice = Some(text.into());
    }

    fn event(&mut self, msg: &Value) -> Result<(), String> {
        if let Some(capture) = self.capture
            && msg["id"] == capture.id
        {
            self.capture = None;
            if capture.viewport_revision == self.viewport_revision
                && let Err(error) = self.decode_frame(capture, &msg["result"])
            {
                self.notice(&error);
            }
            return Ok(());
        }
        if msg["sessionId"]
            .as_str()
            .is_some_and(|session| session != self.session)
            && msg["method"] != "Target.attachedToTarget"
        {
            return Ok(());
        }
        let p = &msg["params"];
        match msg["method"].as_str().unwrap_or_default() {
            "Target.attachedToTarget"
                if p["targetInfo"]["type"] == "page"
                    && p["targetInfo"]["targetId"] != self.target =>
            {
                self.issue(
                    false,
                    "Target.closeTarget",
                    json!({"targetId":p["targetInfo"]["targetId"]}),
                )?;
                self.notice("Pop-up windows are unavailable in Embedded Browser.");
            }
            "Page.windowOpen" => self.notice("Pop-up windows are unavailable in Embedded Browser."),
            "Page.javascriptDialogOpening" => {
                self.issue(true, "Page.handleJavaScriptDialog", json!({"accept":false}))?;
                self.notice("The page opened a dialog. Embedded Browser dismissed it.");
            }
            "Page.fileChooserOpened" => {
                self.issue(
                    true,
                    "DOM.setFileInputFiles",
                    json!({"backendNodeId":p["backendNodeId"],"files":[]}),
                )?;
                self.notice("File selection is unavailable in Embedded Browser.");
            }
            "Browser.downloadWillBegin" | "Page.downloadWillBegin" => {
                self.notice("Downloads are unavailable in Embedded Browser.");
            }
            "Fetch.requestPaused" => {
                let allowed = p["request"]["url"]
                    .as_str()
                    .is_some_and(|url| validated_address(url, true).is_ok());
                let id = &p["requestId"];
                if allowed {
                    self.issue(true, "Fetch.continueRequest", json!({"requestId":id}))?;
                } else {
                    self.issue(
                        true,
                        "Fetch.failRequest",
                        json!({"requestId":id,"errorReason":"BlockedByClient"}),
                    )?;
                    self.notice("Embedded Browser blocked an unsupported address.");
                }
            }
            "Fetch.authRequired" => self.auth_required(p)?,
            "Page.frameStartedLoading" if p["frameId"] == self.main_frame => {
                self.shared.state.lock().unwrap().loading = true;
            }
            "Page.frameStoppedLoading" if p["frameId"] == self.main_frame => {
                self.shared.state.lock().unwrap().loading = false;
            }
            "Page.frameNavigated" if p["frame"]["parentId"].is_null() => {
                if let Some(id) = p["frame"]["id"].as_str() {
                    self.main_frame = id.into();
                }
                // A blocked or failed navigation can commit Chromium's internal error page.
                if p["frame"].get("unreachableUrl").is_none() {
                    self.shared.state.lock().unwrap().error = None;
                }
            }
            "Network.requestWillBeSent"
                if p["type"] == "Document" && p["frameId"] == self.main_frame =>
            {
                self.navigation_request = p["requestId"].as_str().map(str::to_owned);
            }
            "Network.loadingFailed"
                if self.navigation_request.as_deref() == p["requestId"].as_str() =>
            {
                if let Some(code) = p["errorText"].as_str() {
                    self.navigation_error(code);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn navigate(&mut self, value: &str) -> Result<(), String> {
        match validated_address(value, true) {
            Ok(url) => {
                self.cancel_auth()?;
                {
                    let mut state = self.shared.state.lock().unwrap();
                    state.error = None;
                    state.notice = None;
                    state.loading = true;
                    state.address = url.clone();
                }
                // Page.navigate can wait for an HTTP challenge. Keep the main event loop free.
                self.issue(true, "Page.navigate", json!({"url":url}))?;
            }
            Err(error) => self.notice(&error),
        }
        Ok(())
    }

    fn navigation_error(&self, code: &str) {
        let message = if code.starts_with("net::ERR_CERT_") || code.starts_with("net::ERR_SSL_") {
            "Chromium blocked the connection because TLS verification failed."
        } else if code == "net::ERR_ABORTED" {
            return;
        } else {
            "Chromium could not load the requested page."
        };
        let mut state = self.shared.state.lock().unwrap();
        state.error = Some(message.into());
        state.loading = false;
    }

    fn metadata(&mut self) -> Result<(), String> {
        let target = self.call(
            false,
            "Target.getTargetInfo",
            json!({"targetId":self.target}),
        )?;
        let history = self.call(true, "Page.getNavigationHistory", json!({}))?;
        let entries = history["entries"]
            .as_array()
            .ok_or("Invalid browser history response.")?;
        self.history = entries
            .iter()
            .filter_map(|entry| entry["id"].as_i64())
            .collect();
        self.history_index = history["currentIndex"].as_u64().unwrap_or_default() as usize;
        let mut state = self.shared.state.lock().unwrap();
        if let Some(url) = target["targetInfo"]["url"].as_str()
            && validated_address(url, true).is_ok()
            && (!state.loading || url != "about:blank")
        {
            state.address = url.into();
        }
        if let Some(title) = target["targetInfo"]["title"].as_str() {
            state.title = title.chars().take(250).collect();
        }
        state.can_back = self.history_index > 0;
        state.can_forward = self.history_index + 1 < self.history.len();
        Ok(())
    }

    fn request_frame(&mut self) -> Result<(), String> {
        let id = self.issue(
            true,
            "Page.captureScreenshot",
            json!({"format":"jpeg","quality":78,"captureBeyondViewport":false}),
        )?;
        self.capture = Some(Capture {
            id,
            deadline: Instant::now() + REQUEST_TIMEOUT,
            size: self.size,
            viewport_revision: self.viewport_revision,
        });
        Ok(())
    }

    fn decode_frame(&self, capture: Capture, shot: &Value) -> Result<(), String> {
        let encoded = shot["data"]
            .as_str()
            .ok_or("Chromium did not provide a preview.")?;
        if encoded.len() > 8 * 1024 * 1024 {
            return Err("Chromium preview exceeded its size limit.".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "Chromium returned an invalid preview.")?;
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1920);
        limits.max_image_height = Some(1080);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        let rgba = reader
            .decode()
            .map_err(|_| "Could not decode Chromium preview.")?
            .into_rgba8();
        // After the resize is acknowledged, the compositor can still return
        // the previous surface. Only the requested geometry is interactive.
        if [rgba.width(), rgba.height()] == capture.size {
            *self.shared.frame.lock().unwrap() = Some(Frame {
                generation: self.generation,
                viewport_revision: capture.viewport_revision,
                size: capture.size,
                rgba: rgba.into_raw(),
            });
        }
        Ok(())
    }

    fn resize(&mut self, size: [u32; 2], revision: u64) -> Result<(), String> {
        let [width, height] = bounded_size(size);
        self.size = [width, height];
        self.viewport_revision = revision;
        self.shared.frame.lock().unwrap().take();
        self.call(
            false,
            "Browser.setContentsSize",
            json!({"windowId":self.window,"width":width,"height":height}),
        )?;
        #[cfg(target_os = "macos")]
        {
            // AppKit clamps the native window width. Chromium needs both the
            // CSS viewport and the visible surface sized for matching input
            // coordinates and screenshot pixels.
            self.call(
                true,
                "Emulation.setDeviceMetricsOverride",
                json!({"width":width,"height":height,"deviceScaleFactor":1,"mobile":false}),
            )?;
            self.call(
                true,
                "Emulation.setVisibleSize",
                json!({"width":width,"height":height}),
            )?;
        }
        Ok(())
    }

    fn auth_required(&mut self, params: &Value) -> Result<(), String> {
        self.cancel_auth()?;
        let request = params["requestId"]
            .as_str()
            .ok_or("Invalid Chromium authentication challenge.")?;
        let id = self.next;
        self.next += 1;
        let source = &params["authChallenge"];
        let origin = source["origin"]
            .as_str()
            .and_then(|value| validated_address(value, false).ok())
            .and_then(|value| url::Url::parse(&value).ok())
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_else(|| "Unknown origin".into());
        self.auth = Some((id, request.into()));
        self.shared.state.lock().unwrap().challenge = Some(Challenge {
            id,
            proxy: source["source"] == "Proxy",
            origin,
        });
        Ok(())
    }

    fn finish_auth(
        &mut self,
        challenge: u64,
        credentials: Option<Credentials>,
    ) -> Result<(), String> {
        if !self.auth.as_ref().is_some_and(|(id, _)| *id == challenge) {
            return Ok(());
        }
        let (_, request) = self.auth.take().unwrap();
        self.shared.state.lock().unwrap().challenge = None;
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct AuthResponse<'a> {
            response: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            username: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            password: Option<&'a str>,
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'a> {
            request_id: &'a str,
            auth_challenge_response: AuthResponse<'a>,
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Message<'a> {
            id: u64,
            method: &'static str,
            session_id: &'a str,
            params: Params<'a>,
        }
        let response = match credentials.as_ref() {
            Some(credentials) => AuthResponse {
                response: "ProvideCredentials",
                username: Some(credentials.username.as_str()),
                password: Some(credentials.password.as_str()),
            },
            None => AuthResponse {
                response: "CancelAuth",
                username: None,
                password: None,
            },
        };
        let id = self.next;
        self.next += 1;
        self.pipe.send(&Message {
            id,
            method: "Fetch.continueWithAuth",
            session_id: &self.session,
            params: Params {
                request_id: &request,
                auth_challenge_response: response,
            },
        })?;
        Ok(())
    }

    fn cancel_auth(&mut self) -> Result<(), String> {
        if let Some((id, _)) = self.auth.as_ref() {
            self.finish_auth(*id, None)?;
        }
        Ok(())
    }

    fn pointer(
        &mut self,
        kind: &str,
        button: &str,
        x: f64,
        y: f64,
        click_count: u8,
    ) -> Result<(), String> {
        let mask = match button {
            "left" => 1,
            "right" => 2,
            "middle" => 4,
            _ => 0,
        };
        match kind {
            "mousePressed" => self.buttons |= mask,
            "mouseReleased" => self.buttons &= !mask,
            _ => {}
        }
        self.issue(
            true,
            "Input.dispatchMouseEvent",
            json!({"type":kind,"button":button,"x":x,"y":y,"clickCount":click_count,"buttons":self.buttons}),
        )?;
        Ok(())
    }

    fn handle(&mut self, command: Command) -> Result<(), String> {
        match command {
            Command::Navigate(value) => self.navigate(&value)?,
            Command::History(delta) => {
                self.cancel_auth()?;
                if let Some(id) = self
                    .history_index
                    .checked_add_signed(delta as isize)
                    .and_then(|index| self.history.get(index))
                {
                    self.issue(true, "Page.navigateToHistoryEntry", json!({"entryId":id}))?;
                }
            }
            Command::Reload => {
                self.cancel_auth()?;
                self.issue(true, "Page.reload", json!({}))?;
            }
            Command::Stop => {
                self.cancel_auth()?;
                self.issue(true, "Page.stopLoading", json!({}))?;
                self.shared.state.lock().unwrap().loading = false;
            }
            Command::Resize { size, revision } => {
                if revision > self.viewport_revision {
                    self.resize(size, revision)?;
                }
            }
            Command::Pointer { kind, button, x, y } => self.pointer(kind, button, x, y, 1)?,
            Command::CancelPointer => {
                // Chromium treats a move with no buttons as a release
                // outside the page, clearing capture without a click.
                self.buttons = 0;
                self.pointer("mouseMoved", "none", -1., -1., 0)?;
            }
            Command::Wheel { x, y, dx, dy } => {
                self.issue(
                    true,
                    "Input.dispatchMouseEvent",
                    json!({"type":"mouseWheel","x":x,"y":y,"deltaX":dx,"deltaY":dy}),
                )?;
            }
            Command::Key {
                down,
                key,
                code,
                virtual_key,
                modifiers,
                repeat,
            } => {
                self.issue(true, "Input.dispatchKeyEvent", json!({
                    "type":if down {"rawKeyDown"} else {"keyUp"},
                    "key":key,"code":code,"windowsVirtualKeyCode":virtual_key,"modifiers":modifiers,"autoRepeat":repeat
                }))?;
            }
            Command::Text(text) => {
                self.issue(true, "Input.insertText", json!({"text":text}))?;
            }
            Command::Composition {
                text,
                selection: [start, end],
            } => {
                self.issue(
                    true,
                    "Input.imeSetComposition",
                    json!({
                        "text":text,"selectionStart":start,"selectionEnd":end
                    }),
                )?;
            }
            Command::CopySelection => {
                let selection = self.call(true, "Runtime.evaluate", json!({
                    "expression":"(() => { const e = document.activeElement; if (e && e.type !== 'password' && Number.isInteger(e.selectionStart)) return e.value.slice(e.selectionStart, e.selectionEnd).slice(0, 65536); return String(getSelection()).slice(0, 65536); })()",
                    "returnByValue":true,"timeout":500,
                }))?;
                if let Some(value) = selection["result"]["value"].as_str() {
                    *self.shared.copy.lock().unwrap() = Some(value.chars().take(65536).collect());
                }
            }
            Command::Auth {
                challenge,
                credentials,
            } => self.finish_auth(challenge, credentials)?,
        }
        Ok(())
    }
}
