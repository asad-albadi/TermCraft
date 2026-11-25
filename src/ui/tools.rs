#[cfg(windows)]
#[path = "windows_platform.rs"]
mod windows_platform;
use eframe::egui::{
    self, Button, ColorImage, RichText, ScrollArea, TextEdit, TextureHandle, TextureOptions, Ui,
};
use harbor_workspace::tools::capture::{
    CaptureConfig, CaptureHandle, CaptureInterface, CaptureReport, PacketCapture, StopReason,
    enumerate_interfaces,
};
use harbor_workspace::tools::{
    Change, DiffResult, FileDiff, FolderDiff, FolderDiffResult, ImagePreview, PortScan, PortStatus,
    ScanHandle, ScanReport, ScanTarget,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::Duration,
};

#[derive(Default)]
pub(super) struct UiState {
    #[cfg(windows)]
    windows: windows_platform::State,
    comparison: Comparison,
    image: Preview,
    scan: Scan,
    capture: Capture,
}

impl UiState {
    pub(super) fn render(&mut self, ui: &mut Ui) {
        #[cfg(windows)]
        self.windows.render(ui);
        self.comparison.poll();
        self.image.poll(ui);
        self.scan.poll();
        self.capture.poll();
        ui.collapsing("Compare files or folders", |ui| self.comparison.render(ui));
        ui.collapsing("Image viewer", |ui| self.image.render(ui));
        ui.collapsing("TCP port check", |ui| self.scan.render(ui));
        ui.collapsing("Packet capture", |ui| self.capture.render(ui));
        if self.comparison.pending.is_some()
            || self.image.pending.is_some()
            || self.scan.pending.is_some()
            || self.capture.busy()
        {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }
}

#[derive(Default)]
struct Comparison {
    folders: bool,
    left: Option<PathBuf>,
    right: Option<PathBuf>,
    pending: Option<Receiver<Result<ComparisonResult, String>>>,
    cancel: Option<Arc<AtomicBool>>,
    result: Option<Result<ComparisonResult, String>>,
}

enum ComparisonResult {
    Files(DiffResult),
    Folders(FolderDiffResult),
}

impl Comparison {
    fn poll(&mut self) {
        if let Some(result) = take_ready(&mut self.pending) {
            self.cancel = None;
            self.result = Some(result);
        }
    }

    fn render(&mut self, ui: &mut Ui) {
        let busy = self.pending.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            let folders = self.folders;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.folders, false, "Files");
                ui.selectable_value(&mut self.folders, true, "Folders");
            });
            if folders != self.folders {
                self.left = None;
                self.right = None;
                self.result = None;
            }
            let left_changed = select_path(ui, "Choose left…", &mut self.left, self.folders);
            let right_changed = select_path(ui, "Choose right…", &mut self.right, self.folders);
            if left_changed || right_changed {
                self.result = None;
            }
            let ready = self.left.is_some() && self.right.is_some();
            if ui.add_enabled(ready, Button::new("Compare")).clicked()
                && let (Some(left), Some(right)) = (self.left.clone(), self.right.clone())
            {
                let folders = self.folders;
                self.result = None;
                let cancel = Arc::new(AtomicBool::new(false));
                let worker_cancel = Arc::clone(&cancel);
                match start_job("local-comparison", move || {
                    if folders {
                        FolderDiff::compare_with_cancel(&left, &right, &|| {
                            worker_cancel.load(Ordering::Relaxed)
                        })
                        .map(ComparisonResult::Folders)
                    } else {
                        FileDiff::compare(&left, &right).map(ComparisonResult::Files)
                    }
                }) {
                    Ok(pending) => {
                        self.pending = Some(pending);
                        self.cancel = folders.then_some(cancel);
                    }
                    Err(error) => self.result = Some(Err(error)),
                }
            }
        });
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                if let Some(cancel) = &self.cancel {
                    if cancel.load(Ordering::Relaxed) {
                        ui.label("Cancelling…");
                    } else if ui.button("Cancel").clicked() {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            });
        }
        match self.result.as_ref() {
            Some(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            Some(Ok(ComparisonResult::Files(diff))) => render_file_diff(ui, diff),
            Some(Ok(ComparisonResult::Folders(diff))) => render_folder_diff(ui, diff),
            None => {}
        }
    }
}

fn select_path(ui: &mut Ui, button: &str, selected: &mut Option<PathBuf>, folders: bool) -> bool {
    ui.horizontal(|ui| {
        let mut changed = false;
        if ui.button(button).clicked() {
            let dialog = rfd::FileDialog::new();
            let path = if folders {
                dialog.pick_folder()
            } else {
                dialog.pick_file()
            };
            if let Some(path) = path {
                *selected = Some(path);
                changed = true;
            }
        }
        if let Some(path) = selected {
            ui.label(path.to_string_lossy().as_ref());
        }
        changed
    })
    .inner
}

fn render_file_diff(ui: &mut Ui, diff: &DiffResult) {
    if diff.equal {
        ui.label("Files are identical.");
        return;
    }
    let Some(changes) = diff.changes.as_ref() else {
        ui.label("Files differ. A line-by-line preview is available for small UTF-8 text files.");
        return;
    };
    ScrollArea::both()
        .id_salt("file-comparison")
        .max_height(280.)
        .show(ui, |ui| {
            for change in changes {
                let (prefix, text, color) = match change {
                    Change::Added(text) => ("+", text, ui.visuals().selection.stroke.color),
                    Change::Removed(text) => ("−", text, ui.visuals().error_fg_color),
                };
                let line = text.strip_suffix('\n').unwrap_or(text);
                let line = match line.char_indices().nth(500) {
                    Some((end, _)) => format!("{}…", line[..end].replace('\r', "␍")),
                    None => line.replace('\r', "␍"),
                };
                let eof = if text.ends_with('\n') {
                    ""
                } else {
                    "  ⟨no line ending⟩"
                };
                ui.label(
                    RichText::new(format!("{prefix} {line}{eof}"))
                        .monospace()
                        .color(color),
                );
            }
        });
}

fn render_folder_diff(ui: &mut Ui, diff: &FolderDiffResult) {
    let count = diff.added.len() + diff.removed.len() + diff.changed.len();
    if count == 0 {
        ui.label("Folders are identical.");
        return;
    }
    let groups = [
        ("Only on the right", &diff.added),
        ("Only on the left", &diff.removed),
        ("Changed", &diff.changed),
    ];
    for (label, paths) in groups {
        if paths.is_empty() {
            continue;
        }
        ui.collapsing(format!("{label} ({})", paths.len()), |ui| {
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            ScrollArea::vertical()
                .id_salt(("folder-comparison", label))
                .max_height(200.)
                .show_rows(ui, row_height, paths.len(), |ui, rows| {
                    for path in &paths[rows] {
                        let display = path.to_string_lossy();
                        ui.add(egui::Label::new(RichText::new(&*display).monospace()).truncate())
                            .on_hover_text(display);
                    }
                });
        });
    }
}

#[derive(Default)]
struct Preview {
    path: Option<PathBuf>,
    pending: Option<Receiver<Result<ImagePreview, String>>>,
    result: Option<Result<TextureHandle, String>>,
}

impl Preview {
    fn poll(&mut self, ui: &mut Ui) {
        let Some(result) = take_ready(&mut self.pending) else {
            return;
        };
        self.result = Some(result.and_then(|image| {
            let size = [image.width as usize, image.height as usize];
            let max_side = ui.input(|input| input.max_texture_side);
            if size.iter().any(|&side| side > max_side) {
                return Err("Image dimensions exceed this display's texture limit".into());
            }
            let pixels = ColorImage::from_rgba_unmultiplied(size, &image.rgba);
            Ok(ui
                .ctx()
                .load_texture("local-image-preview", pixels, TextureOptions::LINEAR))
        }));
    }

    fn render(&mut self, ui: &mut Ui) {
        if ui
            .add_enabled(self.pending.is_none(), Button::new("Open image…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG and JPEG", &["png", "jpg", "jpeg"])
                .pick_file()
        {
            self.path = Some(path.clone());
            self.result = None;
            match start_job("local-image-preview", move || {
                ImagePreview::open(&path, 4_000_000)
            }) {
                Ok(pending) => self.pending = Some(pending),
                Err(error) => self.result = Some(Err(error)),
            }
        }
        if let Some(path) = &self.path {
            ui.label(path.to_string_lossy().as_ref());
        }
        if self.pending.is_some() {
            ui.spinner();
        }
        match self.result.as_ref() {
            Some(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            Some(Ok(texture)) => {
                let size = texture.size_vec2();
                let scale = (ui.available_width().max(1.) / size.x)
                    .min(480. / size.y)
                    .min(1.);
                ui.label(format!("{} × {} pixels", size.x, size.y));
                ui.add(egui::Image::new(texture).fit_to_exact_size(size * scale));
            }
            None => {}
        }
    }
}

struct Scan {
    address: String,
    ports: String,
    pending: Option<ScanHandle>,
    result: Option<Result<ScanReport, String>>,
}

impl Default for Scan {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".into(),
            ports: String::new(),
            pending: None,
            result: None,
        }
    }
}

impl Scan {
    fn poll(&mut self) {
        if self.pending.as_ref().is_some_and(ScanHandle::is_finished) {
            self.result = self.pending.take().map(ScanHandle::wait);
        }
    }

    fn render(&mut self, ui: &mut Ui) {
        ui.add_enabled_ui(self.pending.is_none(), |ui| {
            ui.horizontal(|ui| {
                ui.label("IP address");
                ui.add(
                    TextEdit::singleline(&mut self.address)
                        .desired_width(125.)
                        .char_limit(45),
                );
                ui.label("TCP ports");
                ui.add(
                    TextEdit::singleline(&mut self.ports)
                        .hint_text("22, 8080")
                        .desired_width(135.)
                        .char_limit(1800),
                );
                if ui.button("Check ports").clicked() {
                    self.result = None;
                    let start = self.target().and_then(PortScan::start);
                    match start {
                        Ok(handle) => self.pending = Some(handle),
                        Err(error) => self.result = Some(Err(error)),
                    }
                }
            });
        });
        if let Some(handle) = &self.pending {
            ui.horizontal(|ui| {
                ui.spinner();
                if ui.button("Cancel").clicked() {
                    handle.cancel();
                }
            });
        }
        match self.result.as_ref() {
            Some(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            Some(Ok(report)) => {
                ui.label(format!("Results for {}", report.address));
                if report.cancelled {
                    ui.label("Cancelled. Showing completed checks.");
                }
                ScrollArea::vertical()
                    .id_salt("local-port-results")
                    .max_height(240.)
                    .show(ui, |ui| {
                        for (port, status) in &report.ports {
                            let label = match status {
                                PortStatus::Open => "Open",
                                PortStatus::Closed => "Closed",
                                PortStatus::TimedOut => "Timed out",
                                PortStatus::Unreachable => "Unreachable",
                            };
                            ui.monospace(format!("{port:>5}  {label}"));
                        }
                    });
            }
            None => {}
        }
    }

    fn target(&self) -> Result<ScanTarget, String> {
        let address = self
            .address
            .trim()
            .parse()
            .map_err(|_| "Enter an IPv4 or IPv6 address")?;
        let ports = self
            .ports
            .split(',')
            .map(|port| port.trim().parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Enter comma-separated TCP ports")?;
        Ok(ScanTarget { address, ports })
    }
}

struct Capture {
    interfaces: Vec<CaptureInterface>,
    interface: Option<String>,
    interface_job: Option<Receiver<Result<Vec<CaptureInterface>, String>>>,
    interface_error: Option<String>,
    filter: String,
    output: Option<PathBuf>,
    snaplen: u32,
    seconds: u64,
    mib: u64,
    packets: u64,
    pending: Option<CaptureHandle>,
    stopping: bool,
    result: Option<Result<CaptureReport, String>>,
}

impl Default for Capture {
    fn default() -> Self {
        Self {
            interfaces: Vec::new(),
            interface: None,
            interface_job: None,
            interface_error: None,
            filter: String::new(),
            output: None,
            snaplen: 65_535,
            seconds: 30,
            mib: 16,
            packets: 10_000,
            pending: None,
            stopping: false,
            result: None,
        }
    }
}

impl Capture {
    fn busy(&self) -> bool {
        self.interface_job.is_some() || self.pending.is_some()
    }

    fn poll(&mut self) {
        if let Some(result) = take_ready(&mut self.interface_job) {
            match result {
                Ok(interfaces) => {
                    if !interfaces
                        .iter()
                        .any(|item| Some(&item.name) == self.interface.as_ref())
                    {
                        self.interface = None;
                    }
                    self.interfaces = interfaces;
                    if self.interfaces.is_empty() {
                        self.interface_error = Some("No capture interfaces found".into());
                    }
                }
                Err(error) => {
                    self.interfaces.clear();
                    self.interface = None;
                    self.interface_error = Some(error);
                }
            }
        }
        if self
            .pending
            .as_ref()
            .is_some_and(CaptureHandle::is_finished)
        {
            self.result = self.pending.take().map(CaptureHandle::wait);
            self.stopping = false;
        }
    }

    fn render(&mut self, ui: &mut Ui) {
        ui.add_enabled_ui(!self.busy(), |ui| {
            ui.horizontal(|ui| {
                if ui.button("List capture interfaces").clicked() {
                    self.interface_error = None;
                    match start_job("capture-interfaces", enumerate_interfaces) {
                        Ok(job) => self.interface_job = Some(job),
                        Err(error) => self.interface_error = Some(error),
                    }
                }
                egui::ComboBox::from_id_salt("packet-capture-interface")
                    .selected_text(self.interface.as_deref().unwrap_or("Choose an interface"))
                    .show_ui(ui, |ui| {
                        for item in &self.interfaces {
                            let row = ui.selectable_value(
                                &mut self.interface,
                                Some(item.name.clone()),
                                &item.name,
                            );
                            if let Some(description) = &item.description {
                                row.on_hover_text(description);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Capture filter");
                ui.add(
                    TextEdit::singleline(&mut self.filter)
                        .hint_text("Optional BPF filter, e.g. udp port 53")
                        .char_limit(4096)
                        .desired_width(260.),
                );
            });
            egui::Grid::new("packet-capture-limits").show(ui, |ui| {
                ui.label("Snapshot bytes");
                ui.add(egui::DragValue::new(&mut self.snaplen).range(1..=262_144));
                ui.label("Time limit (seconds)");
                ui.add(egui::DragValue::new(&mut self.seconds).range(1..=3600));
                ui.end_row();
                ui.label("File limit (MiB)");
                ui.add(egui::DragValue::new(&mut self.mib).range(1..=1024));
                ui.label("Packet limit");
                ui.add(egui::DragValue::new(&mut self.packets).range(1..=1_000_000));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button("Choose PCAP output…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_file_name("capture.pcap")
                        .add_filter("PCAP", &["pcap"])
                        .save_file()
                {
                    self.output = Some(path);
                }
                if let Some(path) = &self.output {
                    ui.add(egui::Label::new(path.to_string_lossy().as_ref()).truncate())
                        .on_hover_text(path.to_string_lossy());
                }
            });
            let ready = self.interface.is_some() && self.output.is_some();
            if ui
                .add_enabled(ready, Button::new("Start capture"))
                .clicked()
                && let (Some(interface), Some(output)) =
                    (self.interface.clone(), self.output.clone())
            {
                self.result = None;
                let config = CaptureConfig {
                    interface,
                    filter: self.filter.clone(),
                    output,
                    snaplen: self.snaplen,
                    max_duration: Duration::from_secs(self.seconds),
                    max_bytes: self.mib * 1024 * 1024,
                    max_packets: self.packets,
                };
                match PacketCapture::start(config) {
                    Ok(handle) => self.pending = Some(handle),
                    Err(error) => self.result = Some(Err(error)),
                }
            }
        });
        if let Some(error) = &self.interface_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.interface_job.is_some() {
            ui.spinner();
        }
        if let Some(handle) = &self.pending {
            let progress = handle.progress();
            ui.horizontal(|ui| {
                ui.spinner();
                if !progress.ready {
                    ui.label("Opening capture interface…");
                } else {
                    ui.label(format!(
                        "{} packets · {} bytes",
                        progress.packets, progress.bytes
                    ));
                }
                if self.stopping {
                    ui.label("Stopping and saving…");
                } else if ui.button("Stop and save").clicked() {
                    self.stopping = true;
                    handle.cancel();
                }
            });
        }
        match self.result.as_ref() {
            Some(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            Some(Ok(report)) => {
                let reason = match report.reason {
                    StopReason::Cancelled => "Stopped",
                    StopReason::TimeLimit => "Time limit reached",
                    StopReason::ByteLimit => "File limit reached",
                    StopReason::PacketLimit => "Packet limit reached",
                };
                let data = &report.metadata;
                ui.label(format!(
                    "{reason}. Saved {} packets · {} bytes · link type {}",
                    data.packets, data.bytes, data.linktype
                ));
                ui.label(report.output.to_string_lossy().as_ref());
            }
            None => {}
        }
    }
}

fn start_job<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<Receiver<Result<T, String>>, String> {
    let (send, receive) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let _ = send.send(work());
        })
        .map_err(|error| format!("Could not start tool: {error}"))?;
    Ok(receive)
}

fn take_ready<T>(pending: &mut Option<Receiver<Result<T, String>>>) -> Option<Result<T, String>> {
    let result = match pending.as_ref()?.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Disconnected) => Err("Tool stopped before producing a result".into()),
        Err(TryRecvError::Empty) => return None,
    };
    *pending = None;
    Some(result)
}
