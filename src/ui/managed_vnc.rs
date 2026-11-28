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
    wayland: bool,
    authority: String,
    #[cfg(target_os = "macos")]
    macos_display_id: u32,
    bind: String,
    browser: bool,
    browser_bind: String,
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
            wayland: false,
            authority: String::new(),
            #[cfg(target_os = "macos")]
            macos_display_id: 0,
            bind: "127.0.0.1:5900".into(),
            browser: false,
            browser_bind: "127.0.0.1:5800".into(),
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
    pub(super) fn active(&self) -> bool {
        self.id
            .and_then(|id| self.supervisor.status(id))
            .is_some_and(|s| {
                matches!(
                    s.state,
                    ServiceState::Starting | ServiceState::Running | ServiceState::Stopping
                )
            })
    }

    pub(super) fn card(&mut self, ui: &mut Ui) {
        ui.add_space(17.);
        surface(ui).show(ui,|ui|{
            ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new("VNC desktop sharing server").size(18.)));
            ui.label("Share an explicitly selected display. Classic VNC uses weak password authentication and does not encrypt screen contents or input. Use an SSH tunnel on untrusted networks.");
            let status=self.id.and_then(|id|self.supervisor.status(id));
            let busy=status.as_ref().is_some_and(|s|matches!(s.state,ServiceState::Starting|ServiceState::Running|ServiceState::Stopping));
            ui.add_enabled_ui(!busy,|ui|{
                fluent::settings_grid(ui, "managed-vnc-form", |ui|{
                    #[cfg(target_os="linux")]{ui.label("Display source");ui.horizontal_wrapped(|ui|{ui.selectable_value(&mut self.wayland,false,"Explicit X11 display");ui.selectable_value(&mut self.wayland,true,"Wayland portal selection");});ui.end_row();if self.wayland {ui.label("Permission");ui.label("Start opens your desktop permission dialog. Select one screen or window. Stop cancels sharing.");ui.end_row();}else{ui.label("X11 display");ui.add(crate::ui::fluent::singleline(&mut self.display).hint_text("Explicit display, e.g. :2"));ui.end_row();ui.label("Xauthority file (optional)");ui.add(crate::ui::fluent::singleline(&mut self.authority));ui.end_row();}}
                    #[cfg(windows)]{ui.label("Display source");ui.label("Primary display on the current Windows desktop");ui.end_row();}
                    #[cfg(target_os="macos")]{ui.label("Display source");ui.add(DragValue::new(&mut self.macos_display_id).prefix("Display ID ").range(0..=u32::MAX));ui.end_row();}
                    ui.label("Listen address");ui.add(crate::ui::fluent::singleline(&mut self.bind));ui.end_row();
                    ui.label("Browser viewer");ui.checkbox(&mut self.browser,"Enable HTTP browser viewer (unencrypted)");ui.end_row();
                    if self.browser {ui.label("Browser listen address");ui.add(crate::ui::fluent::singleline(&mut self.browser_bind));ui.end_row();}
                    let label=ui.label("VNC password");super::managed_servers::service_password_field(ui,&mut self.password,210.).labelled_by(label.id);ui.end_row();
                    ui.label("Access");ui.checkbox(&mut self.control,"Allow remote keyboard and pointer (view-only by default)");ui.end_row();
                    ui.label("Clipboard");
                    #[cfg(target_os="macos")]ui.checkbox(&mut self.clipboard,"Share my global macOS clipboard (viewers receive only)");
                    #[cfg(not(target_os="macos"))]if self.wayland {ui.label("Unavailable for Wayland portal sharing");}else{ui.checkbox(&mut self.clipboard,"Share selected display clipboard (read-only viewers can receive only)");}ui.end_row();
                    ui.label("Network access");ui.checkbox(&mut self.network,"Allow non-loopback listening");ui.end_row();
                    ui.label("Transport");ui.checkbox(&mut self.consent,"I accept unencrypted classic VNC for this listener");ui.end_row();
                    ui.label("Stop after");ui.add(DragValue::new(&mut self.lifetime).range(0..=86400).suffix(" seconds (0 = until stopped)"));ui.end_row();
                });
                #[cfg(target_os="macos")]ui.small("macOS 14 or later. Display ID 0 selects the primary display when Start is clicked; another ID selects only that display. Capture is limited to 5 frames/second. Start may request Screen Recording permission; remote control also requires Accessibility permission. Approve permissions yourself in macOS. The optional clipboard belongs to your entire macOS session.");
                ui.menu_button("Viewer and clipboard help", |ui| { ui.set_max_width(340.); ui.small("Password: 1–8 printable ASCII bytes. Clipboard is limited to 8 KiB text and disabled by default. Browser viewer uses bundled noVNC, shares this server’s password/access, and stops with it. HTTPS is not available on this listener. Wayland sharing requires a desktop portal and PipeWire; permission is requested only when you start this source. One controlling client at a time; view-only servers allow multiple viewers."); });
            });
            if let Some(s)=status {
                ui.label(format!("{:?}",s.state));
                if s.state==ServiceState::Running {if let Some(address)=s.address{ui.monospace(address.to_string());}}
                if s.state==ServiceState::Running { if let Some(mut address)=s.browser_address { if address.ip().is_unspecified() { address.set_ip(if address.is_ipv4() { std::net::Ipv4Addr::LOCALHOST.into() } else { std::net::Ipv6Addr::LOCALHOST.into() }); } let url=format!("http://{address}/");ui.hyperlink_to(&url,&url);ui.small("Use this listener’s literal interface IP for access from another computer. Custom hostnames and reverse proxy origins are not accepted."); } }
                if let Some(error)=s.error{crate::ui::errors::show(ui, crate::ui::errors::Domain::Vnc, error);}
                if busy {ui.ctx().request_repaint_after(Duration::from_millis(50));if ui.button("Stop VNC server").clicked(){let _=self.supervisor.stop(s.id);}}
            }
            if !busy&&ui.button("Start VNC server").clicked(){
                let result=(||{
                    #[cfg(target_os="linux")]let source=if self.wayland {VncSource::WaylandPortal}else{VncSource::X11{display:self.display.clone()}};
                    #[cfg(windows)]let source=VncSource::WindowsPrimaryDesktop;
                    #[cfg(target_os="macos")]let source=VncSource::MacOsDisplay{display_id:self.macos_display_id};
                    #[cfg(not(any(target_os="linux",target_os="macos",windows)))]let source=VncSource::MacOsDisplay{display_id:0};
                    let mut config=VncServiceConfig::local(source,Secret::new(self.password.as_str()));
                    config.bind=self.bind.parse().map_err(|_|"Enter a listen IP address and port.".to_string())?;
                    config.browser_bind=if self.browser {Some(self.browser_bind.parse().map_err(|_|"Enter a browser listen IP address and port.".to_string())?)} else {None};
                    config.x11_authority=(!self.authority.is_empty()).then(||self.authority.clone().into());config.allow_non_loopback=self.network;config.allow_unencrypted=self.consent;config.allow_control=self.control;config.allow_clipboard=self.clipboard&&!self.wayland;config.lifetime=(self.lifetime!=0).then(||Duration::from_secs(u64::from(self.lifetime)));
                    self.supervisor.begin_vnc(config)
                })();self.password.zeroize();match result{Ok(id)=>{self.id=Some(id);self.error=None},Err(e)=>self.error=Some(e)}
            }
            if let Some(error)=&self.error{crate::ui::errors::show(ui, crate::ui::errors::Domain::Vnc, error);}
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
        assert!(!form.wayland);
        assert!(!form.browser);
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
