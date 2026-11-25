#![cfg(any(target_os = "linux", windows))]

#[path = "support/browser_fixture.rs"]
mod browser_fixture;

use browser_fixture::{Fixture, Response};
use harbor_workspace::browser::{Command, Config, Credentials, Session};
use std::time::{Duration, Instant};

#[cfg(windows)]
#[test]
fn windows_browser_bootstrap_entry() {
    if let Some(code) = harbor_workspace::browser::windows_bootstrap() {
        std::process::exit(code);
    }
}

#[cfg(target_os = "linux")]
fn chromium_available() -> bool {
    if std::env::var_os("TERMCRAFT_TEST_BUNDLED_BROWSER").is_some() {
        harbor_workspace::runtime::RuntimeCatalog::for_current_executable()
            .unwrap()
            .resolve(harbor_workspace::runtime::ToolId::BundledChromium)
            .unwrap();
        return true;
    }
    let present = std::path::Path::new("/usr/lib/chromium/chromium").is_file();
    assert!(present || std::env::var_os("TERMCRAFT_REQUIRE_CHROMIUM").is_none());
    present
}

#[cfg(windows)]
fn chromium_available() -> bool {
    use harbor_workspace::runtime::{RuntimeCatalog, ToolId};
    RuntimeCatalog::for_current_executable()
        .and_then(|catalog| catalog.resolve(ToolId::Browser))
        .expect("Native Windows fixture requires installed Chromium or Google Chrome");
    true
}

#[cfg(windows)]
struct WaitForBrowserWorkers;

#[cfg(windows)]
impl Drop for WaitForBrowserWorkers {
    fn drop(&mut self) {
        harbor_workspace::browser::wait_for_shutdown();
    }
}

fn browser(generation: u64, address: String, proxy: String) -> Session {
    let runtime = if std::env::var_os("TERMCRAFT_TEST_BUNDLED_BROWSER").is_some() {
        harbor_workspace::browser::Runtime::BundledChromium
    } else {
        harbor_workspace::browser::Runtime::SystemChromium
    };
    Session::start_with_runtime(
        Config {
            generation,
            address,
            proxy,
            size: [640, 480],
        },
        runtime,
    )
    .unwrap()
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires native Windows and an installed Chrome or Chromium"
)]
fn chromium_renders_and_submits_browser_input() {
    if !chromium_available() {
        return;
    }
    let fixture = Fixture::http(|request| match request.target.as_str() {
        "/redirect" => Response {
            status: "302 Found",
            header: "Location: /fixture\r\n",
            body: "",
        },
        path if path.starts_with("/submit") => Response::html(
            r#"<!doctype html><title>Submitted</title>
            <body style="margin:0;background:lime">
            <button style="position:fixed;right:4px;bottom:4px;width:40px;height:30px;border:0;background:blue"
                    onclick="reportViewport()" aria-label="Report viewport"></button>
            <script>
              let restored = false;
              addEventListener("pageshow", event => { restored = event.persisted; });
              function reportViewport() {
                const {clientWidth, clientHeight} = document.documentElement;
                fetch("/viewport?width=" + clientWidth + "&height=" + clientHeight +
                      "&dpr=" + devicePixelRatio + "&restored=" + restored);
              }
            </script>"#,
        ),
        _ => Response::html(
            "<title>Fixture</title><body style='margin:0;background:red'>\
             <form action='/submit'><input name='note' autofocus style='width:140px;height:30px'>\
             <button style='position:absolute;left:180px;top:5px;width:100px;height:30px'>Send</button></form>",
        ),
    });
    #[cfg(windows)]
    let _cleanup = WaitForBrowserWorkers;
    let session = browser(9, format!("{}/redirect", fixture.origin), String::new());
    wait_until(&session, || {
        let state = session.state();
        state.title == "Fixture"
            && state.address.ends_with("/fixture")
            && session.take_frame().is_some_and(|frame| {
                let pixel = 4 * (640 * 100 + 100);
                frame.generation == 9
                    && frame.size == [640, 480]
                    && frame.rgba[pixel] > 200
                    && frame.rgba[pixel + 1] < 40
            })
    });
    session.send(Command::Text("fixture ✓".into())).unwrap();
    for kind in ["mousePressed", "mouseReleased"] {
        session
            .send(Command::Pointer {
                kind,
                button: "left",
                x: 220.,
                y: 15.,
            })
            .unwrap();
    }
    wait_until(&session, || session.state().title == "Submitted");
    assert!(fixture.saw("/submit?note=fixture+%E2%9C%93"));
    session.send(Command::History(-1)).unwrap();
    wait_until(&session, || {
        session.state().title == "Fixture" && session.state().can_forward
    });
    session.send(Command::History(1)).unwrap();
    wait_until(&session, || {
        session.state().title == "Submitted" && session.state().can_back
    });
    for (revision, [width, height]) in [(1, [300, 200]), (2, [460, 320])] {
        session
            .send(Command::Resize {
                size: [width, height],
                revision,
            })
            .unwrap();
        wait_until(&session, || {
            session.take_frame().is_some_and(|frame| {
                if frame.size != [width, height] || frame.viewport_revision != revision {
                    return false;
                }
                let pixel = (4 * (width * (height - 15) + width - 20)) as usize;
                // The responsive button remains at the bottom right after reflow.
                frame.rgba[pixel] < 40 && frame.rgba[pixel + 1] < 40 && frame.rgba[pixel + 2] > 200
            })
        });
        for kind in ["mousePressed", "mouseReleased"] {
            session
                .send(Command::Pointer {
                    kind,
                    button: "left",
                    x: f64::from(width - 20),
                    y: f64::from(height - 15),
                })
                .unwrap();
        }
        let report = format!("/viewport?width={width}&height={height}&dpr=1&restored=true");
        wait_until(&session, || fixture.saw(&report));
    }
    session
        .send(Command::Navigate("file:///etc/passwd".into()))
        .unwrap();
    wait_until(&session, || session.state().notice.is_some());
    assert!(session.state().address.starts_with(&fixture.origin));
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires native Windows and an installed Chrome or Chromium"
)]
fn chromium_cancels_site_auth_without_credentials_and_rejects_untrusted_tls() {
    if !chromium_available() {
        return;
    }
    let fixture = Fixture::http(|request| {
        if request.target == "/auth" {
            Response {
                status: "401 Unauthorized",
                header: "WWW-Authenticate: Basic realm=\"fixture\"\r\n",
                body: "<title>Denied</title>",
            }
        } else {
            Response::html("<title>Other</title>")
        }
    });
    let tls = Fixture::untrusted_https();
    #[cfg(windows)]
    let _cleanup = WaitForBrowserWorkers;
    let session = browser(19, format!("{}/auth", fixture.origin), String::new());
    wait_until(&session, || session.state().challenge.is_some());
    let challenge = session.state().challenge.unwrap();
    assert_eq!(challenge.origin, fixture.origin);
    assert!(!challenge.proxy);
    session
        .send(Command::Auth {
            challenge: challenge.id,
            credentials: None,
        })
        .unwrap();
    wait_until(&session, || {
        session.state().challenge.is_none() && !session.state().loading
    });
    assert!(fixture.saw("/auth"));
    assert!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.header("Authorization").is_none())
    );
    session.send(Command::Navigate(tls.origin.clone())).unwrap();
    wait_until(&session, || session.state().error.is_some());
    assert!(
        session
            .state()
            .error
            .unwrap()
            .contains("TLS verification failed")
    );
    assert!(tls.requests.lock().unwrap().is_empty());
    assert_ne!(session.state().title, "Untrusted");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires native Windows and an installed Chrome or Chromium"
)]
fn chromium_routes_through_configured_proxy_and_signs_in_only_after_consent() {
    if !chromium_available() {
        return;
    }
    let origin = Fixture::http(|_| Response::html("<title>Direct</title>"));
    let proxy = Fixture::http(|request| {
        if !request.target.contains("/through-proxy") {
            return Response {
                status: "404 Not Found",
                header: "",
                body: "",
            };
        }
        if request.header("Proxy-Authorization") == Some("Basic Zml4dHVyZTp0ZXN0LXBhc3M=") {
            Response::html("<title>Via proxy</title>")
        } else {
            Response {
                status: "407 Proxy Authentication Required",
                header: "Proxy-Authenticate: Basic realm=\"test-proxy\"\r\n",
                body: "",
            }
        }
    });
    let url = format!("{}/through-proxy", origin.origin);
    #[cfg(windows)]
    let _cleanup = WaitForBrowserWorkers;
    let session = browser(29, url.clone(), proxy.origin.clone());
    wait_until(&session, || session.state().challenge.is_some());
    let challenge = session.state().challenge.unwrap();
    assert!(challenge.proxy);
    assert_eq!(challenge.origin, proxy.origin);
    assert!(
        proxy
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.header("Proxy-Authorization").is_none())
    );
    session
        .send(Command::Auth {
            challenge: challenge.id,
            credentials: Some(Credentials {
                username: "fixture".to_owned().into(),
                password: "test-pass".to_owned().into(),
            }),
        })
        .unwrap();
    wait_until(&session, || session.state().title == "Via proxy");
    assert!(proxy.saw(&url));
    assert!(origin.requests.lock().unwrap().is_empty());
}

#[track_caller]
fn wait_until(session: &Session, mut condition: impl FnMut() -> bool) {
    // Debug builds verify the complete 429MB bundle before starting Chromium.
    let startup_budget = if std::env::var_os("TERMCRAFT_TEST_BUNDLED_BROWSER").is_some() {
        60
    } else {
        18
    };
    let until = Instant::now() + Duration::from_secs(startup_budget);
    while Instant::now() < until {
        if condition() {
            return;
        }
        assert!(!session.state().closed, "{:?}", session.state().error);
        std::thread::sleep(Duration::from_millis(20));
    }
    let state = session.state();
    panic!(
        "Chromium fixture timed out at {} (loading: {}, error: {:?}, notice: {:?})",
        state.title, state.loading, state.error, state.notice
    );
}

#[test]
fn browser_settings_keep_legacy_launch_and_remove_proxy_credentials() {
    let mut legacy: harbor_workspace::model::Session =
        serde_json::from_str(r#"{"protocol":"Browser","host":"https://example.invalid/"}"#)
            .unwrap();
    assert!(!legacy.browser_options.embedded);
    assert!(
        !serde_json::to_string(&legacy)
            .unwrap()
            .contains("browser_options")
    );

    legacy.browser_options.embedded = true;
    legacy.browser_options.proxy = "http://fixture:fixture-secret@proxy.invalid:8080".into();
    assert!(
        !serde_json::to_string(&legacy)
            .unwrap()
            .contains("fixture-secret")
    );
    assert!(!format!("{:?}", legacy.browser_options).contains("fixture-secret"));
    assert!(harbor_workspace::browser::validated_proxy(&legacy.browser_options.proxy).is_err());

    let imported: harbor_workspace::browser::Options = serde_json::from_str(
        r#"{"embedded":true,"proxy":"http://fixture:fixture-secret@proxy.invalid:8080"}"#,
    )
    .unwrap();
    assert!(imported.embedded);
    assert!(!imported.proxy.contains("fixture-secret"));
    assert!(
        Session::start(Config {
            generation: 1,
            address: "https://example.invalid/".into(),
            proxy: imported.proxy,
            size: [640, 480],
        })
        .is_err(),
        "A rejected proxy must not cause a direct connection"
    );
}
