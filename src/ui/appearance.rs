//! Linux winit does not supply a system theme. Read the desktop's standard
//! appearance portal off the UI thread and use it as egui's System fallback.
use super::*;

pub(super) struct DesktopTheme {
    updates: Receiver<Option<Theme>>,
    stop: mpsc::SyncSender<()>,
    current: Option<Theme>,
}

impl DesktopTheme {
    #[cfg(target_os = "linux")]
    pub fn watch(ctx: Context) -> Self {
        let (tx, updates) = mpsc::sync_channel(1);
        let (stop, stopped) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut previous = None;
            loop {
                let theme = read_portal_theme();
                if previous != Some(theme) {
                    match tx.try_send(theme) {
                        Ok(()) => {
                            previous = Some(theme);
                            ctx.request_repaint();
                        }
                        Err(mpsc::TrySendError::Disconnected(_)) => break,
                        Err(mpsc::TrySendError::Full(_)) => {}
                    }
                }
                match stopped.recv_timeout(Duration::from_secs(2)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    _ => break,
                }
            }
        });
        Self {
            updates,
            stop,
            current: None,
        }
    }

    pub fn poll(&mut self) -> Option<Theme> {
        while let Ok(theme) = self.updates.try_recv() {
            self.current = theme;
        }
        self.current
    }
}

impl Drop for DesktopTheme {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
    }
}

#[cfg(target_os = "linux")]
fn portal_theme(value: u32) -> Option<Theme> {
    // org.freedesktop.appearance color-scheme: 0/unknown = no preference.
    match value {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn read_portal_theme() -> Option<Theme> {
    let connection = zbus::blocking::connection::Builder::session()
        .ok()?
        .method_timeout(Duration::from_secs(1))
        .build()
        .ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .ok()?;
    // Read is supported by portal v1 as well as v2. Its historical double
    // variant is unwrapped by Value::downcast_ref, as is the single variant.
    let value: zbus::zvariant::OwnedValue = proxy
        .call("Read", &("org.freedesktop.appearance", "color-scheme"))
        .ok()?;
    portal_theme(value.downcast_ref::<u32>().ok()?)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn portal_values_follow_the_desktop_specification() {
        assert_eq!(portal_theme(1), Some(Theme::Dark));
        assert_eq!(portal_theme(2), Some(Theme::Light));
        assert_eq!(portal_theme(0), None);
        assert_eq!(portal_theme(99), None);
    }

    #[test]
    fn system_theme_updates_do_not_override_explicit_appearance() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(&ctx, None, PathBuf::from("data/portal-theme-test"));
        let (tx, updates) = mpsc::sync_channel(1);
        let (stop, _stopped) = mpsc::sync_channel(1);
        w.desktop_theme = Some(DesktopTheme {
            updates,
            stop,
            current: None,
        });
        for (appearance, portal, expected) in [
            (Appearance::System, Some(Theme::Light), Theme::Light),
            (Appearance::System, Some(Theme::Dark), Theme::Dark),
            (Appearance::Light, Some(Theme::Dark), Theme::Light),
            (Appearance::Dark, Some(Theme::Light), Theme::Dark),
            (Appearance::System, Some(Theme::Light), Theme::Light),
            (Appearance::System, None, Theme::Light),
        ] {
            tx.send(portal).unwrap();
            w.store.preferences.appearance = appearance;
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                w.apply_appearance(ui.ctx());
                assert_eq!(ui.ctx().theme(), expected);
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    #[ignore = "requires a running desktop appearance portal"]
    fn reads_live_desktop_appearance_portal() {
        let actual = read_portal_theme().expect("desktop must expose a light or dark preference");
        println!("Desktop portal appearance: {actual:?}");
    }
}
