use super::*;
pub(super) fn session(ui: &mut Ui, session: &mut Session) {
    use harbor_workspace::protocols::{
        ftp::{FtpDataMode, FtpTlsMode, FtpTransferType},
        ftp_actor::LaunchMode,
    };
    let options = &mut session.ftp_options;
    ui.horizontal_wrapped(|ui| {
        ui.label("FTP engine");
        ui.selectable_value(
            &mut options.launch,
            LaunchMode::Embedded,
            "Embedded file browser",
        );
        ui.selectable_value(
            &mut options.launch,
            LaunchMode::External,
            "Installed client",
        );
    });
    if options.launch == LaunchMode::External {
        ui.small("Uses the installed FTP client. Embedded FTPS and graphical transfer options below are unavailable in this mode.");
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Security");
        ui.selectable_value(&mut options.tls, FtpTlsMode::Explicit, "Explicit TLS");
        ui.selectable_value(&mut options.tls, FtpTlsMode::Implicit, "Implicit TLS");
        ui.selectable_value(&mut options.tls, FtpTlsMode::Plain, "Plain FTP");
    });
    if options.tls == FtpTlsMode::Plain {
        options.protect_data = false;
        ui.small("Plain FTP sends credentials and data without encryption.");
    } else {
        ui.checkbox(&mut options.protect_data, "Encrypt data connections");
        ui.horizontal_wrapped(|ui| {
            ui.label("Certificate authority");
            if ui.button("Choose PEM…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    options.certificate_authority = Some(path);
                }
            }
            if options.certificate_authority.is_some() && ui.button("Use system trust").clicked() {
                options.certificate_authority = None;
            }
        });
        if let Some(path) = &options.certificate_authority {
            ui.label(path.display().to_string());
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Data connection");
        ui.selectable_value(&mut options.data_mode, FtpDataMode::Passive, "Passive");
        ui.selectable_value(&mut options.data_mode, FtpDataMode::Active, "Active");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Transfer type");
        ui.selectable_value(
            &mut options.transfer_type,
            FtpTransferType::Binary,
            "Binary (byte exact)",
        );
        ui.selectable_value(
            &mut options.transfer_type,
            FtpTransferType::Ascii,
            "ASCII text conversion",
        );
    });
    ui.small("Graphical browsing requires MLSD support. Passwords are entered privately in the FTP pane. FTP server rename and conflict semantics can differ; verify changes before replacing a file.");
}
