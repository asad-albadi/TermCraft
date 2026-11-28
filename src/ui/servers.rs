use super::*;
use harbor_workspace::services::ServiceProtocol;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Section {
    #[default]
    Http,
    Ssh,
    Ftp,
    Tftp,
    Nfs,
    Telnet,
    Vnc,
    Iperf,
    Cron,
    Xserver,
}
impl Section {
    pub(super) const ALL: [Self; 10] = [
        Self::Http,
        Self::Ssh,
        Self::Ftp,
        Self::Tftp,
        Self::Nfs,
        Self::Telnet,
        Self::Vnc,
        Self::Iperf,
        Self::Cron,
        Self::Xserver,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Ssh => "SSH / SFTP",
            Self::Ftp => "FTP",
            Self::Tftp => "TFTP",
            Self::Nfs => "NFS",
            Self::Telnet => "Telnet",
            Self::Vnc => "VNC",
            Self::Iperf => "Iperf",
            Self::Cron => "Scheduled commands",
            Self::Xserver => "X server",
        }
    }
}
impl Workspace {
    fn service_active(&self, section: Section) -> bool {
        match section {
            Section::Http => self.http.is_some() || self.http_start.is_some(),
            Section::Ftp => self.managed_servers.active(ServiceProtocol::Ftp),
            Section::Tftp => self.managed_servers.active(ServiceProtocol::Tftp),
            Section::Nfs => self.managed_servers.active(ServiceProtocol::Nfs),
            Section::Telnet => self.managed_servers.active(ServiceProtocol::Telnet),
            Section::Ssh => self.managed_ssh_iperf.active(true),
            Section::Iperf => self.managed_ssh_iperf.active(false),
            Section::Vnc => self.managed_vnc.active(),
            Section::Cron => self.managed_cron.active(),
            Section::Xserver => self.xserver.active(),
        }
    }
    pub(super) fn servers_page(&mut self, ui: &mut Ui) {
        ui.heading("Servers");
        ui.horizontal_wrapped(|ui| {
            ui.label("Service");
            ComboBox::from_id_salt("service-section").selected_text(self.servers_category.label()).show_ui(ui,|ui| {
                for section in Section::ALL {
                    let label = if self.service_active(section) { format!("{} · active",section.label()) } else { section.label().to_owned() };
                    ui.selectable_value(&mut self.servers_category,section,label);
                }
            });
            ui.menu_button("Help",|ui| {
                ui.set_max_width(300.);
                ui.label("Choose a service to configure it. Active services keep running when you switch views; select them again to stop them.");
                ui.label("Use SSH / SFTP for encrypted file sharing. FTP, HTTP, Telnet, TFTP, NFS and classic VNC require a trusted network or an encrypted tunnel.");
            });
        });
        ui.separator();
        let section = self.servers_category;
        fluent::page_scroll(ui, &format!("server-{section:?}"), |ui| {
            ui.push_id(section.label(), |ui| {
                let data = self
                    .store_path
                    .parent()
                    .unwrap_or(std::path::Path::new("."));
                match section {
                    Section::Http => self.http_server_card(ui),
                    Section::Ftp => self.managed_servers.card(ui, ServiceProtocol::Ftp),
                    Section::Tftp => self.managed_servers.card(ui, ServiceProtocol::Tftp),
                    Section::Nfs => self.managed_servers.card(ui, ServiceProtocol::Nfs),
                    Section::Telnet => self.managed_servers.card(ui, ServiceProtocol::Telnet),
                    Section::Ssh => self.managed_ssh_iperf.ssh_card(ui, data),
                    Section::Iperf => self.managed_ssh_iperf.iperf_card(ui),
                    Section::Vnc => self.managed_vnc.card(ui),
                    Section::Cron => self.managed_cron.card(ui, data),
                    Section::Xserver => self.xserver_card(ui),
                }
            });
        });
    }
    fn http_server_card(&mut self, ui: &mut Ui) {
        surface(ui).show(ui,|ui|{ui.horizontal_wrapped(|ui|{icon(ui,"folder",24.,accent(ui));ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new("HTTP file server").size(18.)));ui.with_layout(Layout::right_to_left(Align::Center),|ui|{dot(ui,if self.http.is_some(){fluent::palette(ui).success}else{muted(ui)});ui.label(if self.http.is_some(){"Running"}else{"Stopped"});});});
    ui.label(RichText::new("Share files from one folder. Uploads and access-token authentication are opt-in.").color(muted(ui)));ui.add_space(5.);
    ui.add_enabled_ui(self.http.is_none()&&self.http_start.is_none(),|ui|{self.http_share_controls(ui);fluent::settings_grid(ui, "http_config", |ui|{let label=ui.label("Shared folder");ui.horizontal(|ui|{let width=(ui.available_width()-90.).clamp(40.,440.);fluent::text_input(ui,label.id,&mut self.http_root,"",width).fluent_hint(&self.http_root);if ui.button("Browse").clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.http_root=p.to_string_lossy().into();}}});ui.end_row();let label=ui.label("Listen address");fluent::text_input(ui,label.id,&mut self.http_bind,"",260.);ui.end_row();ui.label("Stop after");ui.add(DragValue::new(&mut self.http_timeout).range(0..=86400).suffix(" seconds (0 = until stopped)"));ui.end_row();});});
    if let Some(server)=&self.http{let url=format!("http://{}/",server.address);ui.horizontal_wrapped(|ui|{ui.label(RichText::new(&url).monospace());if ui.button("Copy address").clicked(){ui.ctx().copy_text(url);}});if ui.button("Stop HTTP server").clicked(){self.http=None;self.message("HTTP server stopped");}}
    else if self.http_start.is_some(){ui.label("Starting HTTP sharing…");if ui.button("Cancel HTTP startup").clicked(){self.http_start=None;self.message("HTTP startup cancelled");}}
    else if primary(ui,"Start HTTP server").clicked(){self.start_http_share();}
    if !self.http_log.is_empty(){CollapsingHeader::new("Request log").show(ui,|ui|{ScrollArea::vertical().max_height(150.).show(ui,|ui|{ui.add(Label::new(RichText::new(&self.http_log).monospace().size(12.)).selectable(true));});});}
   });
    }
}
