//! Windows Tools state is ephemeral: registry values are never serialized.
use eframe::egui::{self, Ui};
use harbor_workspace::platform::{
    registry_fs::{ConfirmedWrite, Listing, RegistryFs, RegistryPath, RegistryValue},
    windows_integration::WindowsIntegration,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
enum ResultView {
    Message(String),
    Listing(RegistryPath, Listing),
}
#[derive(Default)]
pub(super) struct State {
    address: String,
    result: Option<Receiver<Result<ResultView, String>>>,
    cancel: Option<Arc<AtomicBool>>,
    message: String,
    message_error: bool,
    listing: Option<(RegistryPath, Listing)>,
    selected: Option<(String, RegistryValue)>,
    edit: String,
    edit_kind: u32,
    preview: Option<ConfirmedWrite>,
    write_enabled: bool,
}
impl State {
    fn start(
        &mut self,
        job: impl FnOnce(Arc<AtomicBool>) -> Result<ResultView, String> + Send + 'static,
    ) {
        if self.result.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = cancel.clone();
        match std::thread::Builder::new()
            .name("windows-tools".into())
            .spawn(move || {
                let result = job(worker.clone());
                let _ = tx.send(result);
            }) {
            Ok(_) => {
                self.result = Some(rx);
                self.cancel = Some(cancel);
                self.message = "Working…".into();
                self.message_error = false;
            }
            Err(e) => {
                self.message = format!("Cannot start Windows operation: {e}");
                self.message_error = true;
            }
        }
    }
    pub(super) fn poll(&mut self) {
        let ready = self.result.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Operation ended or was cancelled".into()))
            }
            Err(_) => None,
        });
        if let Some(value) = ready {
            let cancelled = self
                .cancel
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed));
            self.result = None;
            self.cancel = None;
            self.message_error = false;
            match value {
                Ok(ResultView::Message(s)) => self.message = s,
                Ok(ResultView::Listing(path, listing)) => {
                    self.listing = Some((path, listing));
                    self.selected = None;
                    self.preview = None;
                    self.message = "Read complete".into();
                }
                Err(error) => {
                    self.message_error =
                        !(cancelled && error.to_ascii_lowercase().contains("cancel"));
                    self.message = error;
                }
            }
        }
    }
    fn browse(&mut self) {
        match RegistryPath::parse(&self.address) {
            Ok(path) => {
                self.selected = None;
                self.preview = None;
                self.listing = None;
                self.start(move |cancel| {
                    RegistryFs::enumerate(&path, &cancel)
                        .map(|listing| ResultView::Listing(path, listing))
                });
            }
            Err(error) => {
                self.message = error;
                self.message_error = true;
            }
        }
    }
    pub(super) fn render(&mut self, ui: &mut Ui) {
        self.poll();
        if self.address.is_empty() {
            self.address = "HKCU\\Software".into();
        }
        ui.collapsing("Windows Explorer integration",|ui|{
   ui.label("Opt in for this Windows user: folder menus, termcraft: URLs and .termcraft-link bookmarks. Install from the final executable location. Existing registrations are preserved; no default application choices are changed.");
   ui.add_enabled_ui(self.result.is_none(),|ui|{ui.horizontal(|ui|{
    for (label,action) in [("Status",0),("Install for me",1),("Remove mine",2)]{if ui.button(label).clicked(){self.start(move|cancel|{if cancel.load(Ordering::Relaxed){return Err("Cancelled".into());}let integration=WindowsIntegration::current()?;match action{0=>Ok(ResultView::Message(if integration.status(&cancel)?{"Installed for this executable"}else{"Not fully installed"}.into())),1=>{integration.install_user(&cancel)?;Ok(ResultView::Message("Installed for this user".into()))},_=>{integration.remove_user(&cancel)?;Ok(ResultView::Message("Owned entries removed".into()))}}});}}
   });});
  });
        ui.collapsing("Windows registry browser",|ui|{
   ui.label("Read-only by default. Access denied and limits are shown as errors. Values stay in this view only.");
   ui.add_enabled_ui(self.result.is_none(),|ui|{
    ui.horizontal(|ui|{if ui.text_edit_singleline(&mut self.address).changed(){self.preview=None;self.selected=None;}if ui.button("Browse / refresh").clicked(){self.browse();}});
    ui.checkbox(&mut self.write_enabled,"Enable editing existing registry values");
    let mut navigate=None;let mut choose=None;
    if let Some((path,listing))=&self.listing{
     ui.label(format!("Loaded {:?}\\{}",path.hive,path.key));
     egui::ScrollArea::vertical().id_salt("registry-list").max_height(240.0).show(ui,|ui|{
      for key in &listing.keys {if ui.button(format!("📁 {key}")).clicked(){navigate=Some(format!("{}\\{key}",self.address));}}
      for (name,value) in &listing.values {if ui.selectable_label(self.selected.as_ref().is_some_and(|(n,_)|n==name),format!("{} [{}] {}",if name.is_empty(){"(Default)"}else{name},value.type_name(),value.display().chars().take(256).collect::<String>())).clicked(){choose=Some((name.clone(),value.clone()));}}
     });
    }
    if let Some(address)=navigate{self.address=address;self.browse();}
    if let Some((name,value))=choose{self.edit_kind=value.kind;match value.edit_text(){Ok(text)=>{self.edit=text;self.selected=Some((name,value));},Err(error)=>{self.message=error;self.message_error=true;self.selected=None;}};self.preview=None;}
    if self.write_enabled && let Some((name,old))=self.selected.clone(){
     ui.label(format!("Existing value: {} [{}]",if name.is_empty(){"(Default)"}else{&name},old.type_name()));
     egui::ComboBox::from_id_salt("registry-type").selected_text(RegistryValue{kind:self.edit_kind,bytes:vec![]}.type_name()).show_ui(ui,|ui|{for (kind,label) in [(1,"REG_SZ"),(2,"REG_EXPAND_SZ"),(7,"REG_MULTI_SZ (JSON string array)"),(4,"REG_DWORD (decimal)"),(11,"REG_QWORD (decimal)"),(3,"REG_BINARY (hex bytes)"),(0,"REG_NONE (hex bytes)")]{if ui.selectable_value(&mut self.edit_kind,kind,label).changed(){self.preview=None;}}});
     if ui.add(egui::TextEdit::multiline(&mut self.edit).desired_rows(3)).changed(){self.preview=None;}
     if ui.button("Preview value change").clicked(){match RegistryValue::parse_text(self.edit_kind,&self.edit){Ok(replacement)=>{if let Some((path,_))=&self.listing{self.preview=Some(ConfirmedWrite{path:path.clone(),name,expected:old,replacement});}},Err(error)=>{self.message=error;self.message_error=true;}}}
    }
   });
   if self.write_enabled && let Some(preview)=self.preview.clone(){
    crate::ui::fluent::surface(ui).show(ui, |ui| {ui.strong(format!("Confirm target: {:?}\\{} → {}",preview.path.hive,preview.path.key,if preview.name.is_empty(){"(Default)"}else{&preview.name}));
    egui::ScrollArea::vertical().id_salt("registry-change-details").max_height(160.).show(ui, |ui| {ui.label(format!("Before [{}]: {}",preview.expected.type_name(),format!("{:?}",preview.expected.edit_text().unwrap_or_else(|_|preview.expected.display()))));ui.label(format!("After [{}]: {}",preview.replacement.type_name(),format!("{:?}",preview.replacement.edit_text().unwrap_or_else(|_|preview.replacement.display()))));});
    ui.label("Writing changes this value immediately. A changed value will be refused; refresh and preview again.");
    if ui.add_enabled(self.result.is_none(),egui::Button::new("Confirm write to this target and value")).clicked(){self.preview=None;self.start(move|cancel|{RegistryFs::write_confirmed(&preview,&cancel)?;Ok(ResultView::Message("Value written; refresh to read it again".into()))});}
    if ui.button("Discard preview").clicked(){self.preview=None;}
    });
   }
  });
        if self.message_error {
            crate::ui::errors::show(ui, crate::ui::errors::Domain::Tools, &self.message);
        } else {
            ui.label(&self.message);
        }
        if self.result.is_some() {
            if ui.button("Cancel / stop waiting").clicked() {
                if let Some(cancel) = &self.cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
                self.message_error = false;
                self.message="Cancellation requested. A registry write already applied cannot be undone by cancellation.".into();
            }
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }
}
