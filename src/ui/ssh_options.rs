use super::*;
use harbor_workspace::model::{ProxyKind, ProxySettings, SshDefaults, SshOptions};
use harbor_workspace::ssh::config::ResolvedSshOptions;
#[derive(Default)]
struct Preview {
    running: bool,
    result: Option<(String, Result<ResolvedSshOptions, String>)>,
}
pub(super) fn session(ui: &mut Ui, s: &mut Session, defaults: &SshDefaults) {
    CollapsingHeader::new("Advanced SSH and proxy settings").show(ui, |ui| {
        let options = &mut s.ssh_options;
        optional_seconds(ui, "Keepalive interval", &mut options.keepalive_secs, defaults.keepalive_secs, 86400);
        ui.checkbox(&mut options.keepalive_disabled, "Disable keepalive");
        optional_seconds(ui, "Connect timeout", &mut options.connect_timeout_secs, defaults.connect_timeout_secs, 3600);
        fluent::form_row(ui,"Maximum jump hops",|ui,label|ui.add(DragValue::new(&mut options.jump_limit).range(1..=4)).labelled_by(label));
        ui.small("Enter comma-separated jump hosts in connection order. Each hop verifies its own host key.");
        let mut config = options.config_file.is_some();
        if ui.checkbox(&mut config, "Read an OpenSSH config file").changed() {
            options.config_file = config.then(PathBuf::new);
        }
        if let Some(path)=&mut options.config_file {
            let mut text=path.to_string_lossy().into_owned();
            super::pages::session_path(ui, "SSH config path", &mut text, "", rfd::FileDialog::pick_file);
            if text != path.to_string_lossy() { *path=PathBuf::from(text); }
            ui.checkbox(&mut options.prefer_config,"Prefer config identity fields (embedded SSH)");
            ui.small("HostName resolves the alias. Otherwise session identity fields take precedence; blank user/key/jump fields inherit config values. Timer overrides take precedence over config, then global defaults.");
        }
        let mut proxy=options.proxy.is_some();
        if ui.checkbox(&mut proxy,"Connect through a TCP proxy").changed() {
            options.proxy=proxy.then(||ProxySettings{kind:ProxyKind::HttpConnect,host:String::new(),port:8080,credential_ref:None,authenticate:false});
        }
        if let Some(proxy)=&mut options.proxy {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut proxy.kind,ProxyKind::HttpConnect,"HTTP CONNECT");
                ui.selectable_value(&mut proxy.kind,ProxyKind::Socks5,"SOCKS5");
            });
            fluent::form_text(ui, "Proxy host", &mut proxy.host, "Proxy hostname");
            fluent::form_row(ui,"Proxy port",|ui,label|ui.add(DragValue::new(&mut proxy.port).range(1..=65535)).labelled_by(label));
            ui.checkbox(&mut proxy.authenticate,"Require proxy sign-in");
            ui.small("Proxy sign-in happens before SSH encryption; use a trusted proxy/network. Credentials can be saved in the vault from the sign-in prompt. Typed proxy settings require embedded SSH.");
        }
        if let Err(message)=options.validate(defaults) { crate::ui::errors::show(ui,crate::ui::errors::Domain::Ssh,message); }
        let fingerprint=serde_json::to_string(&(s.clone(),defaults)).unwrap_or_default();
        let state=ui.ctx().data_mut(|data|data.get_temp_mut_or_default::<std::sync::Arc<std::sync::Mutex<Preview>>>(Id::new("ssh-config-preview")).clone());
        let mut preview=state.lock().unwrap_or_else(|p|p.into_inner());
        if ui.add_enabled(!preview.running,Button::new(if preview.running {"Resolving…"}else{"Preview effective SSH configuration"})).clicked() {
            preview.running=true;preview.result=None;
            let session=s.clone();let defaults=defaults.clone();let fingerprint=fingerprint.clone();let state=state.clone();let ctx=ui.ctx().clone();
            std::thread::spawn(move|| {let result=SshOptions::resolve(&session,&defaults);let mut state=state.lock().unwrap_or_else(|p|p.into_inner());state.result=Some((fingerprint,result));state.running=false;ctx.request_repaint();});
        }
        if let Some((bound,result))=&preview.result { if bound==&fingerprint {match result {
            Ok(resolved)=>{
                ui.label(format!("{}@{}:{} · {} gateway(s) · timeout {}s · keepalive {}",resolved.session.user,resolved.session.host,resolved.session.port,resolved.gateways.len(),resolved.connect_timeout_secs,resolved.keepalive_secs.map(|n|format!("{n}s")).unwrap_or_else(||"disabled".into())));
                for field in &resolved.report.fields {ui.small(format!("{} · {}:{} · {}",field.field,field.source,field.line,if field.applied {"candidate"}else{"session/earlier value takes precedence"}));}
                for field in &resolved.report.unsupported {crate::ui::errors::show(ui,crate::ui::errors::Domain::Ssh,format!("Unsupported: {} · {}:{}",field.field,field.source,field.line));}
                if resolved.report.requires_compatibility() {ui.label("Choose OpenSSH compatibility to use this file's native semantics.");if ui.button("Use OpenSSH compatibility").clicked(){s.ssh_engine=SshEngine::OpenSsh;s.ssh_options.prefer_config=false;}}
            },Err(message)=>{crate::ui::errors::show(ui,crate::ui::errors::Domain::Ssh,message);}
        }}else{ui.small("Settings changed; preview again for current values.");}}
    });
}
fn optional_seconds(ui: &mut Ui, label: &str, value: &mut Option<u32>, default: u32, max: u32) {
    ui.horizontal_wrapped(|ui| {
        let mut enabled = value.is_some();
        if ui
            .checkbox(&mut enabled, format!("Override {label}"))
            .changed()
        {
            *value = enabled.then_some(default);
        }
        if let Some(value) = value {
            ui.add(DragValue::new(value).range(1..=max).suffix("s"));
        } else {
            ui.small(format!("Global/config ({default}s global)"));
        }
    });
}
