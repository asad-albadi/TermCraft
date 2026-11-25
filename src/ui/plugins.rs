use super::*;
use harbor_workspace::plugins::{InstalledPlugin, PluginRegistry, ReviewedPlugin};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

#[derive(Default)]
pub(super) struct State {
    initialized: bool,
    items: Vec<InstalledPlugin>,
    review: Option<ReviewedPlugin>,
    approval: Option<InstalledPlugin>,
    job: Option<Job>,
    status: Option<Result<String, String>>,
}

struct Job {
    label: &'static str,
    receiver: Receiver<Reply>,
    cancelled: Arc<AtomicBool>,
    cancellable: bool,
}

struct Reply {
    outcome: Result<Change, String>,
    items: Result<Vec<InstalledPlugin>, String>,
}

enum Change {
    Refresh,
    Reviewed(ReviewedPlugin),
    Installed(String),
    Status(String),
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Workspace {
    pub(super) fn plugins_section(&mut self, ui: &mut Ui) {
        let registry = PluginRegistry::new(self.store_path.with_file_name("plugins"));
        self.plugins.render(ui, registry);
    }

    pub(super) fn poll_plugins(&mut self, ctx: &Context) {
        self.plugins.poll(ctx);
    }

    pub(super) fn stop_plugin_job(&mut self) {
        if let Some(job) = self.plugins.job.take() {
            job.cancelled.store(true, Ordering::Release);
            if job.label == "Running plugin…" {
                let _ = job.receiver.recv_timeout(Duration::from_secs(2));
            }
        }
    }
}

impl State {
    fn poll(&mut self, ctx: &Context) {
        let Some(job) = &self.job else { return };
        let reply = match job.receiver.try_recv() {
            Ok(reply) => reply,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = Some(Err("Plugin operation ended unexpectedly.".into()));
                self.job = None;
                return;
            }
        };
        self.job = None;
        self.status = match reply.outcome {
            Ok(Change::Reviewed(review)) => {
                self.review = Some(review);
                None
            }
            Ok(Change::Installed(status)) => {
                self.review = None;
                Some(Ok(status))
            }
            Ok(Change::Status(status)) => Some(Ok(status)),
            Ok(Change::Refresh) => None,
            Err(error) => Some(Err(error)),
        };
        match reply.items {
            Ok(items) => self.items = items,
            Err(error) if self.status.as_ref().is_none_or(Result::is_ok) => {
                self.status = Some(Err(error));
            }
            Err(_) => {}
        }
        ctx.request_repaint();
    }

    fn start(
        &mut self,
        ui: &Ui,
        registry: PluginRegistry,
        label: &'static str,
        cancellable: bool,
        operation: impl FnOnce(&PluginRegistry, &AtomicBool) -> Result<Change, String> + Send + 'static,
    ) {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let context = ui.ctx().clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        match thread::Builder::new()
            .name("plugin-operation".into())
            .spawn(move || {
                let outcome = operation(&registry, &flag);
                let items = registry.list().map_err(|error| error.to_string());
                let _ = sender.send(Reply { outcome, items });
                context.request_repaint();
            }) {
            Ok(_) => {
                self.job = Some(Job {
                    label,
                    receiver,
                    cancelled,
                    cancellable,
                });
                self.status = None;
            }
            Err(_) => self.status = Some(Err("Could not start the plugin operation.".into())),
        }
    }

    fn render(&mut self, ui: &mut Ui, registry: PluginRegistry) {
        self.poll(ui.ctx());
        ui.collapsing("Plugins", |ui| {
            if !matches!((std::env::consts::OS, std::env::consts::ARCH), ("linux" | "windows", "x86_64")) {
                ui.label("TermCraft plugins are available on x86-64 Linux and Windows.");
                return;
            }
            if !self.initialized {
                self.initialized = true;
                self.start(ui, registry.clone(), "Loading plugins…", false, |_, _| Ok(Change::Refresh));
            }
            if let Some(result) = &self.status {
                match result {
                    Ok(status) => { ui.label(status); }
                    Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error); }
                }
            }
            if let Some(job) = &self.job {
                ui.horizontal(|ui| {
                    ui.spinner();
                    let cancelling = job.cancelled.load(Ordering::Acquire);
                    ui.label(if cancelling { "Cancelling…" } else { job.label });
                    if job.cancellable && ui.add_enabled(!cancelling, Button::new("Cancel")).clicked() {
                        job.cancelled.store(true, Ordering::Release);
                    }
                });
                return;
            }
            ui.horizontal(|ui| {
                if ui.button("Inspect plugin bundle…").clicked() {
                    self.review = None;
                    self.approval = None;
                    self.start(ui, registry.clone(), "Inspecting plugin bundle…", true, |registry, cancel| {
                        let path = rfd::FileDialog::new()
                            .set_title("Select a TermCraft plugin manifest")
                            .add_filter("TermCraft plugin manifest", &["json"])
                            .pick_file();
                        let Some(path) = path else { return Ok(Change::Refresh) };
                        registry.inspect(&path, cancel).map(Change::Reviewed).map_err(|error| error.to_string())
                    });
                }
                if ui.button("Refresh").clicked() && self.job.is_none() {
                    self.start(ui, registry.clone(), "Loading plugins…", false, |_, _| Ok(Change::Refresh));
                }
            });
            if self.job.is_some() { return; }
            ui.label("Select a .termcraft-plugin.json manifest. Keep its matching .tar.gz archive alongside it.");

            if let Some(review) = self.review.clone() {
                ui.group(|ui| {
                    let manifest = review.manifest().clone();
                    ui.strong(format!("{} {}", manifest.id, manifest.version));
                    ui.label(format!("Plugin protocol {} · {} {}", manifest.host_protocol, manifest.platform, manifest.architecture));
                    ui.label(format!("Entrypoints · {}", manifest.entrypoints.join(", ")));
                    ui.label("Requested permission · Native execution with full access to your account.");
                    ui.label(format!("Release SHA-256 · {}", review.release_digest()));
                    ui.horizontal(|ui| {
                        let update = self.items.iter().any(|item| item.manifest.id == manifest.id);
                        let label = if update { "Install update disabled" } else { "Install disabled" };
                        if ui.button(label).clicked() {
                            self.start(ui, registry.clone(), "Installing plugin…", true, move |registry, cancel| {
                                let installed = registry.install(&review, cancel).map_err(|error| error.to_string())?;
                                Ok(Change::Installed(format!("{} {} installed and disabled.", installed.manifest.id, installed.manifest.version)))
                            });
                        }
                        if ui.button("Dismiss").clicked() {
                            self.review = None;
                        }
                    });
                });
            }
            if self.job.is_some() { return; }

            if let Some(approval) = self.approval.clone() {
                ui.group(|ui| {
                    ui.strong(format!("Enable {} {}?", approval.manifest.id, approval.manifest.version));
                    ui.label("This native plugin runs as you. It can access your files, credentials and network and start other programs. Grant access only to code you trust.");
                    ui.label(format!("Release SHA-256 · {}", approval.release_digest));
                    ui.horizontal(|ui| {
                        if ui.button("Grant full account access and enable").clicked() {
                            self.approval = None;
                            self.start(ui, registry.clone(), "Enabling plugin…", false, move |registry, _| {
                                registry.enable(&approval.manifest.id, &approval.release_digest)
                                    .map_err(|error| error.to_string())?;
                                Ok(Change::Status(format!("{} enabled.", approval.manifest.id)))
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            self.approval = None;
                        }
                    });
                });
            }
            if self.job.is_some() { return; }
            if self.items.is_empty() {
                ui.label("No plugins installed.");
            }
            for item in self.items.clone() {
                self.render_item(ui, &registry, item);
                if self.job.is_some() { break; }
            }
        });
    }

    fn render_item(&mut self, ui: &mut Ui, registry: &PluginRegistry, item: InstalledPlugin) {
        let id = item.manifest.id.clone();
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let status = if item.enabled { "Enabled" } else { "Disabled" };
                ui.strong(format!("{id} {} · {status}", item.manifest.version));
                if item.enabled {
                    if ui.button("Disable").clicked() {
                        let id = id.clone();
                        self.start(
                            ui,
                            registry.clone(),
                            "Disabling plugin…",
                            false,
                            move |registry, _| {
                                registry.disable(&id).map_err(|error| error.to_string())?;
                                Ok(Change::Status(format!("{id} disabled.")))
                            },
                        );
                    }
                } else if ui.button("Enable…").clicked() {
                    self.approval = Some(item.clone());
                }
                if self.job.is_none() && ui.button("Remove").clicked() {
                    self.approval = None;
                    let id = id.clone();
                    self.start(
                        ui,
                        registry.clone(),
                        "Removing plugin…",
                        true,
                        move |registry, cancel| {
                            let result = registry
                                .remove(&id, cancel)
                                .map_err(|error| error.to_string())?;
                            let message = if result.deferred {
                                format!("{id} removed. Cleanup is pending.")
                            } else if let Some(path) = result.preserved_at {
                                format!(
                                    "{id} removed. User files were preserved in {}.",
                                    path.display()
                                )
                            } else {
                                format!("{id} removed.")
                            };
                            Ok(Change::Status(message))
                        },
                    );
                }
            });
            if item.enabled && self.job.is_none() {
                ui.horizontal_wrapped(|ui| {
                    for entrypoint in item.manifest.entrypoints {
                        if ui.button(format!("Run {entrypoint}")).clicked() {
                            let id = id.clone();
                            let digest = item.release_digest.clone();
                            self.start(
                                ui,
                                registry.clone(),
                                "Running plugin…",
                                true,
                                move |registry, cancel| {
                                    registry
                                        .run_release(&id, &digest, &entrypoint, cancel)
                                        .map_err(|error| error.to_string())?;
                                    Ok(Change::Status(format!("{id} · {entrypoint} completed.")))
                                },
                            );
                            break;
                        }
                    }
                });
            }
        });
    }
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod pointer_workflow_tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path, process::Command};

    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn bundle(root: &Path) -> PathBuf {
        let source = root.join("plugin.c");
        let image = root.join("fixture");
        fs::write(
            &source,
            r#"
#include <stdio.h>
#include <string.h>
#include <unistd.h>
int main(void) {
    char request[256];
    if (!fgets(request, sizeof(request), stdin)) return 1;
    FILE *marker = fopen("invoked", "a");
    if (!marker) return 2;
    fprintf(marker, "%s", request); fclose(marker);
    if (strstr(request, "hang")) for (;;) pause();
    puts("{\"protocol\":1,\"ok\":true}");
    return 0;
}
"#,
        )
        .unwrap();
        assert!(
            Command::new("cc")
                .arg("-o")
                .arg(&image)
                .arg(&source)
                .status()
                .unwrap()
                .success()
        );
        let image = fs::read(image).unwrap();
        let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (name, bytes) in [
            ("bin/fixture", image.as_slice()),
            ("notices/LICENSE", b"UI fixture".as_slice()),
        ] {
            let mut header = tar::Header::new_ustar();
            header.set_mode(0o755);
            header.set_size(bytes.len() as u64);
            header.set_cksum();
            archive.append_data(&mut header, name, bytes).unwrap();
        }
        let archive = archive.into_inner().unwrap().finish().unwrap();
        fs::write(root.join("fixture.tar.gz"), &archive).unwrap();
        let manifest = root.join("fixture.termcraft-plugin.json");
        fs::write(&manifest, serde_json::to_vec(&serde_json::json!({
            "schema": 1, "id": "fixture", "version": "1.0.0", "host_protocol": 1,
            "platform": "linux", "architecture": "x86_64", "archive": "fixture.tar.gz",
            "archive_sha256": digest(&archive), "binary_sha256": digest(&image),
            "executable": "fixture", "entrypoints": ["check", "hang"], "permissions": ["native-execution"]
        })).unwrap()).unwrap();
        manifest
    }

    fn text_position(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|shape| text_position(shape, label))
            }
            _ => None,
        }
    }

    fn frame(ctx: &Context, workspace: &mut Workspace, events: Vec<Event>) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1500., 1600.))),
                events,
                ..Default::default()
            },
            |ui| workspace.render(ui),
        );
        output.textures_delta.clear();
        output
    }

    fn painted(ctx: &Context, workspace: &mut Workspace, label: &str) -> Option<Pos2> {
        frame(ctx, workspace, vec![])
            .shapes
            .iter()
            .find_map(|item| {
                text_position(&item.shape, label).filter(|point| item.clip_rect.contains(*point))
            })
    }

    fn click(ctx: &Context, workspace: &mut Workspace, label: &str) {
        let point = (0..12)
            .find_map(|_| painted(ctx, workspace, label))
            .unwrap_or_else(|| panic!("Missing painted Tools control {label:?}"));
        for pressed in [true, false] {
            frame(
                ctx,
                workspace,
                vec![
                    Event::PointerMoved(point),
                    Event::PointerButton {
                        pos: point,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn wait_for(ctx: &Context, workspace: &mut Workspace, ready: impl Fn(&Workspace) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            frame(ctx, workspace, vec![]);
            if ready(workspace) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "plugin UI did not complete its worker: {:?}",
                workspace.plugins.status
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn tools_plugin_pointer_consent_invocation_cancel_disable_keeps_live_pty_usable() {
        let ctx = Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().join("workspace"));
        workspace.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Session::default()
            },
            Some(connection::Launch {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "while IFS= read -r value; do printf 'ui-pty-%s\\n' \"$value\"; done".into(),
                ],
                directory: None,
                rdp_profile: None,
            }),
        );
        workspace.page = Page::Tools;
        let plugin_root = workspace.store_path.with_file_name("plugins");
        let registry = PluginRegistry::new(plugin_root.clone());
        // Supply the result of the native picker/inspection boundary; every
        // lifecycle action below uses a pointer on the actual rendered Tools UI.
        workspace.plugins.review = Some(
            registry
                .inspect(&bundle(root.path()), &AtomicBool::new(false))
                .unwrap(),
        );
        click(&ctx, &mut workspace, "Plugins");
        wait_for(&ctx, &mut workspace, |w| {
            w.plugins.initialized && w.plugins.job.is_none()
        });
        assert!(
            painted(
                &ctx,
                &mut workspace,
                "Requested permission · Native execution with full access to your account."
            )
            .is_some()
        );
        click(&ctx, &mut workspace, "Install disabled");
        wait_for(&ctx, &mut workspace, |w| {
            w.plugins.job.is_none() && !w.plugins.items.is_empty()
        });
        assert!(!registry.list().unwrap()[0].enabled);
        let release = fs::read_dir(plugin_root.join("versions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let marker = release.join("invoked");
        assert!(!marker.exists());
        assert!(painted(&ctx, &mut workspace, "Run check").is_none());

        click(&ctx, &mut workspace, "Enable…");
        assert!(painted(&ctx, &mut workspace, "This native plugin runs as you. It can access your files, credentials and network and start other programs. Grant access only to code you trust.").is_some());
        assert!(!registry.list().unwrap()[0].enabled);
        click(&ctx, &mut workspace, "Cancel");
        assert!(workspace.plugins.approval.is_none());
        assert!(!registry.list().unwrap()[0].enabled);
        assert!(!marker.exists());
        click(&ctx, &mut workspace, "Enable…");
        click(&ctx, &mut workspace, "Grant full account access and enable");
        wait_for(&ctx, &mut workspace, |w| {
            w.plugins.job.is_none() && w.plugins.items[0].enabled
        });
        assert!(!marker.exists(), "granting consent executed the plugin");
        click(&ctx, &mut workspace, "Run check");
        wait_for(&ctx, &mut workspace, |w| w.plugins.job.is_none());
        assert_eq!(
            workspace.plugins.status,
            Some(Ok("fixture · check completed.".into()))
        );
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);

        click(&ctx, &mut workspace, "Run hang");
        wait_for(&ctx, &mut workspace, |_| {
            fs::read_to_string(&marker).is_ok_and(|s| s.lines().count() == 2)
        });
        assert!(workspace.plugins.job.is_some());
        assert!(
            painted(&ctx, &mut workspace, "Run check").is_none(),
            "busy UI allowed another invocation"
        );
        workspace.tabs[0].send(b"during-plugin\n");
        wait_for(&ctx, &mut workspace, |w| {
            w.tabs[0].screen.plain().contains("ui-pty-during-plugin")
        });
        click(&ctx, &mut workspace, "Cancel");
        wait_for(&ctx, &mut workspace, |w| w.plugins.job.is_none());
        assert!(workspace.plugins.status.as_ref().is_some_and(|status| {
            status
                .as_ref()
                .is_err_and(|error| error.contains("cancelled"))
        }));
        click(&ctx, &mut workspace, "Disable");
        wait_for(&ctx, &mut workspace, |w| {
            w.plugins.job.is_none() && !w.plugins.items[0].enabled
        });
        assert!(!registry.list().unwrap()[0].enabled);
        assert!(painted(&ctx, &mut workspace, "Run check").is_none());
        assert!(painted(&ctx, &mut workspace, "Run hang").is_none());
        workspace.tabs[0].send(b"after-disable\n");
        wait_for(&ctx, &mut workspace, |w| {
            w.tabs[0].screen.plain().contains("ui-pty-after-disable")
        });
        assert_eq!(
            fs::read_to_string(marker).unwrap().lines().count(),
            2,
            "disabled UI launched the plugin"
        );
        workspace.force_close_tab(0);
    }
}
