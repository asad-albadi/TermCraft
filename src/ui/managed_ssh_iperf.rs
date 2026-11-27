use super::*;
use harbor_workspace::{
    credentials::Secret,
    runtime::RuntimeCatalog,
    services::{
        IperfServiceConfig, IperfVersion, ServiceId, ServiceState, ServiceSupervisor,
        SshAuthorizedUser, SshServiceConfig,
    },
};
use std::path::Path;
use zeroize::{Zeroize, Zeroizing};
pub(super) struct ManagedSshIperf {
    supervisor: ServiceSupervisor,
    ssh: SshForm,
    iperf: IperfForm,
}
struct SshForm {
    bind: String,
    root: String,
    user: String,
    password: Zeroizing<String>,
    keys: String,
    write: bool,
    network: bool,
    sftp: bool,
    shell_enabled: bool,
    shell: String,
    lifetime: u32,
    id: Option<ServiceId>,
    error: Option<String>,
}
struct IperfForm {
    bind: String,
    version: IperfVersion,
    network: bool,
    installed: bool,
    interval: u8,
    udp: bool,
    lifetime: u32,
    id: Option<ServiceId>,
    error: Option<String>,
}
impl Default for ManagedSshIperf {
    fn default() -> Self {
        Self {
            supervisor: Default::default(),
            ssh: SshForm {
                bind: "127.0.0.1:2222".into(),
                root: String::new(),
                user: String::new(),
                password: Zeroizing::new(String::new()),
                keys: String::new(),
                write: false,
                network: false,
                sftp: true,
                shell_enabled: false,
                shell: String::new(),
                lifetime: 3600,
                id: None,
                error: None,
            },
            iperf: IperfForm {
                bind: "127.0.0.1:5201".into(),
                version: IperfVersion::Three,
                network: false,
                installed: false,
                interval: 1,
                udp: false,
                lifetime: 3600,
                id: None,
                error: None,
            },
        }
    }
}
impl ManagedSshIperf {
    pub(super) fn cards(&mut self, ui: &mut Ui, data: &Path) {
        surface(ui).show(ui,|ui|{ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new("SSH / SFTP server").size(18.)));ui.label("Embedded encrypted service with a persistent workspace host key. Authorized usernames control service access; they do not create OS accounts.");
            let busy=active(&self.supervisor,self.ssh.id);
            ui.add_enabled_ui(!busy,|ui|{
                fluent::settings_grid(ui, "ssh-service-form", |ui|{
                    ui.label("Listen address");ui.add(crate::ui::fluent::singleline(&mut self.ssh.bind));ui.end_row();
                    ui.label("Network access");ui.checkbox(&mut self.ssh.network,"Allow non-loopback listening");ui.end_row();
                    ui.label("Authorized username");ui.add(crate::ui::fluent::singleline(&mut self.ssh.user));ui.end_row();
                    let label=ui.label("Password (optional with key)");super::managed_servers::service_password_field(ui, &mut self.ssh.password, ui.spacing().text_edit_width).labelled_by(label.id);ui.end_row();
                    ui.label("Authorized public keys");ui.add(TextEdit::multiline(&mut self.ssh.keys).desired_rows(2).hint_text("One OpenSSH public key per line"));ui.end_row();
                    ui.label("File access");ui.checkbox(&mut self.ssh.sftp,"Enable SFTP");ui.end_row();
                    if self.ssh.sftp {ui.label("Shared folder");ui.horizontal_wrapped(|ui|{ui.add(crate::ui::fluent::singleline(&mut self.ssh.root));if ui.button("Browse").clicked(){if let Some(path)=rfd::FileDialog::new().pick_folder(){self.ssh.root=path.to_string_lossy().into_owned();}}});ui.end_row();ui.label("Permissions");ui.checkbox(&mut self.ssh.write,"Allow new uploads, renames and deletes");ui.end_row();}
                    ui.label("Terminal access");ui.checkbox(&mut self.ssh.shell_enabled,"Allow shell with my account’s full permissions");ui.end_row();
                    if self.ssh.shell_enabled {ui.label("Shell executable");ui.add(crate::ui::fluent::singleline(&mut self.ssh.shell));ui.end_row();}
                    ui.label("Stop after");ui.add(DragValue::new(&mut self.ssh.lifetime).range(0..=86400).suffix(" seconds (0 = until stopped)"));ui.end_row();
                });
                ui.small("Uploads create new files (64 MiB maximum); existing files are never overwritten. Shell access has your full account authority and is not restricted to the shared folder.");
            });
            if controls(ui,&self.supervisor,self.ssh.id,&self.ssh.error,"SSH / SFTP") {
                let result=(||{
                    let mut config=SshServiceConfig::local(data.join("services/ssh-host-key"),self.ssh.root.clone().into());
                    config.bind=self.ssh.bind.parse().map_err(|_|"Enter a listen IP address and port.".to_string())?;config.allow_non_loopback=self.ssh.network;config.allow_write=self.ssh.write;config.sftp_root=self.ssh.sftp.then(||self.ssh.root.clone().into());config.shell=self.ssh.shell_enabled.then(||self.ssh.shell.clone().into());config.lifetime=(self.ssh.lifetime!=0).then(||Duration::from_secs(self.ssh.lifetime as u64));
                    config.users.push(SshAuthorizedUser{username:self.ssh.user.clone(),password:(!self.ssh.password.is_empty()).then(||Secret::new(self.ssh.password.as_str())),public_keys:self.ssh.keys.lines().filter(|s|!s.trim().is_empty()).map(|s|s.trim().to_owned()).collect()});
                    self.supervisor.begin_ssh(config)
                })();self.ssh.password.zeroize();match result{Ok(id)=>{self.ssh.id=Some(id);self.ssh.error=None;},Err(error)=>self.ssh.error=Some(error)}
            }
        });
        ui.add_space(17.);
        surface(ui).show(ui,|ui|{ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new("Iperf bandwidth server").size(18.)));ui.label("Owned foreground Iperf engine. Qualified packaged engines are checksum verified; installed compatibility engines require explicit selection.");
            let busy=active(&self.supervisor,self.iperf.id);ui.add_enabled_ui(!busy,|ui|{
                fluent::settings_grid(ui, "iperf-service-form", |ui|{
                    ui.label("Version");ComboBox::from_id_salt("iperf-version").selected_text(if self.iperf.version==IperfVersion::Three{"Iperf 3"}else{"Iperf 2"}).show_ui(ui,|ui|{for(version,label,bind)in[(IperfVersion::Three,"Iperf 3","127.0.0.1:5201"),(IperfVersion::Two,"Iperf 2","127.0.0.1:5001")]{if ui.selectable_value(&mut self.iperf.version,version,label).clicked(){self.iperf.bind=bind.into();self.iperf.udp=false;}}});ui.end_row();
                    ui.label("Listen address");ui.add(crate::ui::fluent::singleline(&mut self.iperf.bind));ui.end_row();ui.label("Network access");ui.checkbox(&mut self.iperf.network,"Allow non-loopback listening");ui.end_row();ui.label("Provider");ui.checkbox(&mut self.iperf.installed,"Allow installed compatibility engine if packaged engine is absent");ui.end_row();
                    if self.iperf.version==IperfVersion::Two {ui.label("Transport");ui.checkbox(&mut self.iperf.udp,"UDP server");ui.end_row();}
                    ui.label("Report interval");ui.add(DragValue::new(&mut self.iperf.interval).range(1..=60).suffix(" seconds"));ui.end_row();ui.label("Stop after");ui.add(DragValue::new(&mut self.iperf.lifetime).range(0..=86400).suffix(" seconds (0 = until stopped)"));ui.end_row();
                });
                ui.small("Iperf clients must use the selected major version and port. Iperf3 clients choose TCP or UDP. No authentication is provided by this bandwidth service.");
            });
            if controls(ui,&self.supervisor,self.iperf.id,&self.iperf.error,"Iperf") {
                let result=(||{let catalog=RuntimeCatalog::for_current_executable()?;let mut config=IperfServiceConfig::local(self.iperf.version,catalog.service_runtime_root());config.bind=self.iperf.bind.parse().map_err(|_|"Enter a listen IP address and nonzero port.".to_string())?;config.allow_non_loopback=self.iperf.network;config.allow_installed_provider=self.iperf.installed;config.udp=self.iperf.udp;config.interval_seconds=self.iperf.interval;config.lifetime=(self.iperf.lifetime!=0).then(||Duration::from_secs(self.iperf.lifetime as u64));self.supervisor.begin_iperf(config)})();match result{Ok(id)=>{self.iperf.id=Some(id);self.iperf.error=None;},Err(error)=>self.iperf.error=Some(error)}
            }
        });
    }
}
fn active(supervisor: &ServiceSupervisor, id: Option<ServiceId>) -> bool {
    id.and_then(|id| supervisor.status(id)).is_some_and(|s| {
        matches!(
            s.state,
            ServiceState::Starting | ServiceState::Running | ServiceState::Stopping
        )
    })
}
fn controls(
    ui: &mut Ui,
    supervisor: &ServiceSupervisor,
    id: Option<ServiceId>,
    error: &Option<String>,
    name: &str,
) -> bool {
    let status = id.and_then(|id| supervisor.status(id));
    if let Some(status) = &status {
        ui.label(format!("{:?}", status.state));
        if let Some(address) = status
            .address
            .filter(|_| status.state == ServiceState::Running)
        {
            ui.monospace(address.to_string());
        }
        if let Some(error) = &status.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        for event in &status.log {
            ui.small(*event);
        }
    }
    if let Some(error) = error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if active(supervisor, id) {
        ui.ctx().request_repaint_after(Duration::from_millis(125));
        if ui.button(format!("Stop {name}")).clicked() {
            if let Some(id) = id {
                let _ = supervisor.stop(id);
            }
        }
        false
    } else {
        primary(ui, &format!("Start {name}")).clicked()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_service_password_field_leaves_no_undo_history() {
        let data = tempfile::tempdir().unwrap();
        let mut forms = ManagedSshIperf::default();
        super::super::managed_servers::tests::password_history(
            &mut forms,
            |forms, ui| forms.cards(ui, data.path()),
            |forms| {
                assert_eq!(forms.ssh.password.as_str(), "FAKE_SERVICE_PASSWORD");
                forms.ssh.password.zeroize();
            },
            "Password (optional with key)",
        );
    }
    struct Client;
    impl russh::client::Handler for Client {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            _: &russh::keys::PublicKeyOrCertificate,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    fn wait_ssh(
        ctx: &Context,
        work: &mut Workspace,
        expected: ServiceState,
    ) -> std::net::SocketAddr {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            super::super::managed_servers::tests::frame(ctx, work, vec![]);
            let id = work
                .managed_ssh_iperf
                .ssh
                .id
                .expect("pointer Start must own an SSH service");
            let status = work.managed_ssh_iperf.supervisor.status(id).unwrap();
            if status.state == expected {
                return status.address.unwrap();
            }
            assert_ne!(status.state, ServiceState::Failed, "{status:?}");
            assert!(
                Instant::now() < deadline,
                "expected {expected:?}: {status:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    #[test]
    fn servers_ssh_pointer_start_auth_root_readonly_stop_preserves_local_pty() {
        use super::super::managed_servers::tests::{click, frame};
        let ctx = Context::default();
        let data = tempfile::tempdir().unwrap();
        let share = data.path().join("share");
        fs::create_dir(&share).unwrap();
        fs::write(share.join("visible"), b"service-root-fixture").unwrap();
        fs::write(data.path().join("outside"), b"outside-root-fixture").unwrap();
        let mut work = Workspace::new_context(&ctx, None, data.path().to_owned());
        work.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    r#"printf PTY_READY; while read line; do printf 'PTY_%s\n' "$line"; done"#
                        .into(),
                ],
                directory: None,
            }),
        );
        work.page = Page::Servers;
        let form = &mut work.managed_ssh_iperf.ssh;
        form.bind = "127.0.0.1:0".into();
        form.root = share.to_string_lossy().into_owned();
        form.user = "fixture".into();
        *form.password = "FAKE_SERVICE_PASSWORD".into();
        click(&ctx, &mut work, "Start SSH / SFTP");
        assert!(work.managed_ssh_iperf.ssh.password.is_empty());
        let address = wait_ssh(&ctx, &mut work, ServiceState::Running);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(8), async {
                for (user, pass) in [
                    ("unknown", "FAKE_SERVICE_PASSWORD"),
                    ("fixture", "wrong-fixture"),
                ] {
                    let mut denied = russh::client::connect(
                        std::sync::Arc::new(russh::client::Config::default()),
                        address,
                        Client,
                    )
                    .await
                    .unwrap();
                    assert!(
                        !denied
                            .authenticate_password(user, pass)
                            .await
                            .unwrap()
                            .success()
                    );
                }
                let mut client = russh::client::connect(
                    std::sync::Arc::new(russh::client::Config::default()),
                    address,
                    Client,
                )
                .await
                .unwrap();
                assert!(
                    client
                        .authenticate_password("fixture", "FAKE_SERVICE_PASSWORD")
                        .await
                        .unwrap()
                        .success()
                );
                let channel = client.channel_open_session().await.unwrap();
                channel.request_subsystem(true, "sftp").await.unwrap();
                let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
                    .await
                    .unwrap();
                assert_eq!(sftp.read("visible").await.unwrap(), b"service-root-fixture");
                assert!(sftp.read("../outside").await.is_err());
                assert!(sftp.create("forbidden-upload").await.is_err());
            })
            .await
            .expect("SSH UI fixture timed out");
        });
        assert!(!share.join("forbidden-upload").exists());
        work.tabs[0].send(b"BEFORE_STOP\n");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            work.tabs[0].poll();
            if work.tabs[0].screen.plain().contains("PTY_BEFORE_STOP") {
                break;
            }
            assert!(Instant::now() < deadline, "local PTY failed before Stop");
            frame(&ctx, &mut work, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        click(&ctx, &mut work, "Stop SSH / SFTP");
        assert_eq!(wait_ssh(&ctx, &mut work, ServiceState::Stopped), address);
        assert!(std::net::TcpStream::connect(address).is_err());
        let rebound = std::net::TcpListener::bind(address).expect("Stop must release listener");
        work.tabs[0].send(b"AFTER_STOP\n");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            work.tabs[0].poll();
            if work.tabs[0].screen.plain().contains("PTY_AFTER_STOP") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Stop disrupted separate local PTY"
            );
            frame(&ctx, &mut work, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(rebound);
    }
}
