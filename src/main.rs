mod ui;
fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            harbor_workspace::diagnostics::Diagnostic::new(
                harbor_workspace::diagnostics::Context::Startup,
                &error.to_string()
            )
            .report()
        );
        std::process::exit(1);
    }
}
fn run() -> eframe::Result {
    if let Some(code) = harbor_workspace::x11::capability_probe_entry() {
        std::process::exit(code);
    }
    #[cfg(windows)]
    if let Some(code) = harbor_workspace::plugins::windows_bootstrap() {
        std::process::exit(code);
    }
    #[cfg(windows)]
    if let Some(code) = harbor_workspace::browser::windows_bootstrap() {
        std::process::exit(code);
    }
    #[cfg(windows)]
    if let Some(code) =
        harbor_workspace::platform::windows_integration::management_entry(std::env::args().skip(1))
    {
        std::process::exit(code);
    }
    if std::env::args_os().skip(1).eq(["--version"]) {
        println!("TermCraft {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    use harbor_workspace::{
        cli::{Cli, CliAction},
        ipc::{Binding, InstanceBroker},
    };
    let mut invocation = Cli::parse_invocation(std::env::args().skip(1)).unwrap_or_else(|error| {
        eprintln!(
            "{}",
            harbor_workspace::diagnostics::Diagnostic::new(
                harbor_workspace::diagnostics::Context::CommandLine,
                &error.to_string()
            )
            .report()
        );
        std::process::exit(2)
    });
    if matches!(invocation.action, CliAction::Help) {
        print!("{}", harbor_workspace::cli::HELP);
        return Ok(());
    }
    let base = std::env::current_dir().unwrap_or_else(|error| {
        eprintln!(
            "{}",
            harbor_workspace::diagnostics::Diagnostic::new(
                harbor_workspace::diagnostics::Context::Startup,
                &format!("Cannot resolve working directory: {error}")
            )
            .report()
        );
        std::process::exit(1)
    });
    invocation.action.absolute_paths(&base);
    let screenshot = match &invocation.action {
        CliAction::Screenshot { path } => Some(path.to_str().unwrap().to_owned()),
        _ => None,
    };
    let data_dir = invocation.data_dir.unwrap_or_else(|| {
        if screenshot.is_some() {
            harbor_workspace::model::config_dir().join("screenshot-workspace")
        } else {
            harbor_workspace::model::config_dir()
        }
    });
    let data_dir = if data_dir.is_absolute() {
        data_dir
    } else {
        base.join(data_dir)
    };
    let primary = if screenshot.is_none() {
        match InstanceBroker::bind(&data_dir).unwrap_or_else(|error| {
            eprintln!(
                "{}",
                harbor_workspace::diagnostics::Diagnostic::new(
                    harbor_workspace::diagnostics::Context::Startup,
                    &format!("Cannot open workspace dispatcher: {error}")
                )
                .report()
            );
            std::process::exit(1)
        }) {
            Binding::Primary(primary) => Some(primary),
            Binding::Secondary(client) => {
                match client.send(invocation.action) {
                    Ok(receipt) => println!(
                        "Request {} accepted by the existing workspace; results appear in its terminal tabs.",
                        receipt.id
                    ),
                    Err(error) => {
                        eprintln!(
                            "{}",
                            harbor_workspace::diagnostics::Diagnostic::new(
                                harbor_workspace::diagnostics::Context::CommandLine,
                                &format!("Launch request failed: {error}")
                            )
                            .report()
                        );
                        std::process::exit(1)
                    }
                }
                return Ok(());
            }
        }
    } else {
        None
    };
    let logo = harbor_workspace::branding::logo(256);
    let window_icon = eframe::egui::IconData {
        width: logo.width(),
        height: logo.height(),
        rgba: logo.into_raw(),
    };
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1360., 880.])
        .with_min_inner_size([640., 480.])
        .with_title("TermCraft — Remote workspace")
        .with_app_id("termcraft")
        .with_icon(window_icon);
    if screenshot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(eframe::egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    if screenshot.is_some() {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_activate_ignoring_other_apps(false);
        }));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = &mut options;
    eframe::run_native(
        "TermCraft",
        options,
        Box::new(move |cc| {
            let mut workspace = ui::Workspace::new(cc, screenshot, data_dir);
            if let Some(primary) = primary {
                workspace.attach_dispatch(primary, invocation.action);
            }
            Ok(Box::new(workspace))
        }),
    )
}
