use super::*;
use harbor_workspace::{
    credentials::Secret,
    services::{ServiceId, ServiceState, ServiceSupervisor, VncServiceConfig, VncSource},
};
use zeroize::{Zeroize, Zeroizing};
pub(super) struct ManagedVnc {
    supervisor: ServiceSupervisor,
    id: Option<ServiceId>,
    display: String,
    authority: String,
    bind: String,
    password: Zeroizing<String>,
    control: bool,
    clipboard: bool,
    network: bool,
    consent: bool,
    lifetime: u32,
    error: Option<String>,
}
impl Default for ManagedVnc {
    fn default() -> Self {
        Self {
            supervisor: Default::default(),
            id: None,
            display: String::new(),
            authority: String::new(),
            bind: "127.0.0.1:5900".into(),
            password: Zeroizing::new(String::new()),
            control: false,
            clipboard: false,
            network: false,
            consent: false,
            lifetime: 3600,
            error: None,
        }
    }
}
impl ManagedVnc {
    pub(super) fn card(&mut self, ui: &mut Ui) {
        ui.add_space(17.);
        surface(ui).show(ui,|ui|{
            ui.label(RichText::new("VNC desktop sharing server").size(18.).strong());
            ui.label("Share an explicitly selected display. Classic VNC uses weak password authentication and does not encrypt screen contents or input. Use an SSH tunnel on untrusted networks.");
            let status=self.id.and_then(|id|self.supervisor.status(id));
            let busy=status.as_ref().is_some_and(|s|matches!(s.state,ServiceState::Starting|ServiceState::Running|ServiceState::Stopping));
            ui.add_enabled_ui(!busy,|ui|{
                Grid::new("managed-vnc-form").num_columns(2).show(ui,|ui|{
                    #[cfg(target_os="linux")]{ui.label("X11 display");ui.add(TextEdit::singleline(&mut self.display).hint_text("Explicit display, e.g. :2"));ui.end_row();ui.label("Xauthority file (optional)");ui.text_edit_singleline(&mut self.authority);ui.end_row();}
                    #[cfg(windows)]{ui.label("Display source");ui.label("Primary display on the current Windows desktop");ui.end_row();}
                    #[cfg(target_os="macos")]{ui.label("Display source");ui.label("Native macOS sharing is not implemented");ui.end_row();}
                    ui.label("Listen address");ui.text_edit_singleline(&mut self.bind);ui.end_row();
                    ui.label("VNC password");super::managed_servers::service_password_field(ui,&mut self.password,210.);ui.end_row();
                    ui.label("Access");ui.checkbox(&mut self.control,"Allow remote keyboard and pointer (view-only by default)");ui.end_row();
                    ui.label("Clipboard");ui.checkbox(&mut self.clipboard,"Share selected display clipboard (read-only viewers can receive only)");ui.end_row();
                    ui.label("Network access");ui.checkbox(&mut self.network,"Allow non-loopback listening");ui.end_row();
                    ui.label("Transport");ui.checkbox(&mut self.consent,"I accept unencrypted classic VNC for this listener");ui.end_row();
                    ui.label("Stop after");ui.add(DragValue::new(&mut self.lifetime).range(0..=86400).suffix(" seconds (0 = until stopped)"));ui.end_row();
                });
                ui.small("Password: 1–8 printable ASCII bytes. Clipboard is limited to 8 KiB text and disabled by default. Browser HTTP viewer is not implemented. Wayland requires a user-approved portal backend, which is not implemented. One controlling client at a time; view-only servers allow multiple viewers.");
            });
            if let Some(s)=status {
                ui.label(format!("{:?}",s.state));
                if s.state==ServiceState::Running {if let Some(address)=s.address{ui.monospace(address.to_string());}}
                if let Some(error)=s.error{ui.colored_label(ui.visuals().error_fg_color,error);}
                if busy {ui.ctx().request_repaint_after(Duration::from_millis(50));if ui.button("Stop VNC server").clicked(){let _=self.supervisor.stop(s.id);}}
            }
            if !busy&&ui.button("Start VNC server").clicked(){
                let result=(||{
                    #[cfg(target_os="linux")]let source=VncSource::X11{display:self.display.clone()};
                    #[cfg(windows)]let source=VncSource::WindowsPrimaryDesktop;
                    #[cfg(not(any(target_os="linux",windows)))]let source=VncSource::MacOsDisplay{display_id:0};
                    let mut config=VncServiceConfig::local(source,Secret::new(self.password.as_str()));
                    config.bind=self.bind.parse().map_err(|_|"Enter a listen IP address and port.".to_string())?;
                    config.x11_authority=(!self.authority.is_empty()).then(||self.authority.clone().into());config.allow_non_loopback=self.network;config.allow_unencrypted=self.consent;config.allow_control=self.control;config.allow_clipboard=self.clipboard;config.lifetime=(self.lifetime!=0).then(||Duration::from_secs(u64::from(self.lifetime)));
                    self.supervisor.begin_vnc(config)
                })();self.password.zeroize();match result{Ok(id)=>{self.id=Some(id);self.error=None},Err(e)=>self.error=Some(e)}
            }
            if let Some(error)=&self.error{ui.colored_label(ui.visuals().error_fg_color,error);}
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vnc_service_password_field_leaves_no_undo_history() {
        let mut form = ManagedVnc::default();
        assert!(!form.clipboard);
        assert!(!form.control);
        super::super::managed_servers::tests::password_history(
            &mut form,
            |form, ui| form.card(ui),
            |form| {
                assert_eq!(form.password.as_str(), "FAKE_SERVICE_PASSWORD");
                form.password.zeroize();
            },
            "VNC password",
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires private bwrap/Cage display; never run on a host display"]
    fn servers_vnc_pointer_start_stop_owns_only_selected_display() {
        use super::super::managed_servers::tests::{click, frame};
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let ctx = Context::default();
        let data = tempfile::tempdir().unwrap();
        let mut work = Workspace::new_context(&ctx, None, data.path().to_owned());
        work.page = Page::Servers;
        let package =
            std::path::PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE").unwrap());
        let manager = harbor_workspace::x11::XServerManager::for_installation(package);
        let mut lease = manager.start(Default::default()).unwrap();
        work.managed_vnc.display = lease.display().into();
        work.managed_vnc.authority = lease.authority_path().to_string_lossy().into_owned();
        work.managed_vnc.bind = "127.0.0.1:0".into();
        *work.managed_vnc.password = "fixture".into();
        work.managed_vnc.consent = true;
        click(&ctx, &mut work, "Start VNC server");
        let id = work
            .managed_vnc
            .id
            .expect("pointer start must own VNC service");
        let deadline = Instant::now() + Duration::from_secs(5);
        let address = loop {
            frame(&ctx, &mut work, vec![]);
            let status = work.managed_vnc.supervisor.status(id).unwrap();
            assert_ne!(status.state, ServiceState::Failed, "{status:?}");
            if status.state == ServiceState::Running {
                break status.address.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(work.managed_vnc.password.is_empty());
        assert!(
            !serde_json::to_string(&work.store)
                .unwrap()
                .contains("fixture")
        );
        assert!(std::net::TcpStream::connect(address).is_ok());
        click(&ctx, &mut work, "Stop VNC server");
        let deadline = Instant::now() + Duration::from_secs(5);
        while work.managed_vnc.supervisor.status(id).unwrap().state != ServiceState::Stopped {
            frame(&ctx, &mut work, vec![]);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(std::net::TcpStream::connect(address).is_err());
        assert!(lease.is_running().unwrap());
        assert!(std::os::unix::net::UnixStream::connect("/tmp/.X11-unix/X0").is_ok());
    }
}
