use super::*;
impl Workspace {
    pub(super) fn servers_page(&mut self, ui: &mut Ui) {
        pages::page_heading(
            ui,
            "Servers",
            "Share a folder or run a local service while your workspace is open.",
        );
        ScrollArea::vertical().show(ui,|ui|{
   surface(ui).show(ui,|ui|{ui.horizontal_wrapped(|ui|{icon(ui,"folder",24.,accent(ui));ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new("HTTP file server").size(18.)));ui.with_layout(Layout::right_to_left(Align::Center),|ui|{dot(ui,if self.http.is_some(){fluent::palette(ui).success}else{muted(ui)});ui.label(if self.http.is_some(){"Running"}else{"Stopped"});});});
    ui.label(RichText::new("Share files from one folder. Uploads and access-token authentication are opt-in.").color(muted(ui)));ui.add_space(5.);
    ui.add_enabled_ui(self.http.is_none()&&self.http_start.is_none(),|ui|{self.http_share_controls(ui);fluent::settings_grid(ui, "http_config", |ui|{let label=ui.label("Shared folder");ui.horizontal_wrapped(|ui|{fluent::text_input(ui,label.id,&mut self.http_root,"",440.).fluent_hint(&self.http_root);if ui.button("Browse").clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.http_root=p.to_string_lossy().into();}}});ui.end_row();let label=ui.label("Listen address");fluent::text_input(ui,label.id,&mut self.http_bind,"",260.);ui.end_row();ui.label("Stop after");ui.add(DragValue::new(&mut self.http_timeout).range(0..=86400).suffix(" seconds (0 = until stopped)"));ui.end_row();});});
    if let Some(server)=&self.http{let url=format!("http://{}/",server.address);ui.horizontal_wrapped(|ui|{ui.label(RichText::new(&url).monospace());if ui.button("Copy address").clicked(){ui.ctx().copy_text(url);}});if ui.button("Stop HTTP server").clicked(){self.http=None;self.message("HTTP server stopped");}}
    else if self.http_start.is_some(){ui.label("Starting HTTP sharing…");if ui.button("Cancel HTTP startup").clicked(){self.http_start=None;self.message("HTTP startup cancelled");}}
    else if primary(ui,"Start HTTP server").clicked(){self.start_http_share();}
    if !self.http_log.is_empty(){CollapsingHeader::new("Request log").show(ui,|ui|{ScrollArea::vertical().max_height(150.).show(ui,|ui|{ui.add(Label::new(RichText::new(&self.http_log).monospace().size(12.)).selectable(true));});});}
   });ui.add_space(17.);
   self.managed_servers.cards(ui);
   let service_data=self.store_path.parent().unwrap_or(std::path::Path::new("."));
   self.managed_ssh_iperf.cards(ui,service_data);
   self.managed_vnc.card(ui);
   ui.add_space(17.);self.managed_cron.card(ui,service_data);
   ui.add_space(17.);self.xserver_card(ui);
   ui.add_space(17.);CollapsingHeader::new("Additional original server types").show(ui,|ui|{ui.label("VNC offers optional browser viewing and platform-specific sharing. Wayland requires portal permission and the optional PipeWire helper; macOS requires version 14 or later and native permissions. FTPS server support in Personal is still unverified.");});
  });
    }
}
