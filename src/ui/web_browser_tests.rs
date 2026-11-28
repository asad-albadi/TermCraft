use super::*;
use crate::ui::terminal_review_tests::{owned_process_fixture_slot, owned_workspace};

#[allow(dead_code)]
#[path = "../../tests/support/browser_fixture.rs"]
mod fixture;

struct Rendered {
    output: egui::FullOutput,
    images: Vec<(TextureId, egui::ImageData)>,
}

fn render(ctx: &Context, workspace: &mut Workspace, events: Vec<Event>) -> Rendered {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 900.))),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| workspace.render(ui),
    );
    let images = output
        .textures_delta
        .set
        .iter()
        .flat_map(|(id, deltas)| deltas.iter().map(|delta| (*id, delta.image.clone())))
        .collect();
    output.textures_delta.clear();
    Rendered { output, images }
}

#[test]
fn embedded_browser_tab_paints_chromium_and_routes_egui_pointer_and_text() {
    let Some(_chromium_slot) = chromium_fixture_slot() else {
        return;
    };
    let fixture = fixture::Fixture::http(|request| {
        if request.target.starts_with("/accepted") {
            fixture::Response::html("<title>Accepted</title>")
        } else {
            fixture::Response::html(
                "<title>Workspace fixture</title><body style='margin:0;background:fuchsia'>\
                 <form action='/accepted'>\
                 <input name='text' style='position:absolute;left:10px;top:10px;width:160px;height:35px'>\
                 <button style='position:absolute;left:200px;top:10px;width:95px;height:35px'>Send</button>\
                 </form>",
            )
        }
    });
    let (ctx, _root, mut workspace) = owned_workspace();
    workspace.new_session("Browser");
    let mut profile = workspace.session_form.take().unwrap();
    profile.browser_options.runtime = fixture_runtime();
    assert!(profile.browser_options.embedded);
    profile.host = fixture.origin.clone();
    let persisted = serde_json::to_string(&profile).unwrap();
    workspace.start(serde_json::from_str(&persisted).unwrap());
    let id = workspace.tabs[workspace.active].id;
    let deadline = Instant::now() + fixture_budget(18);
    let (canvas, [width, height]) = loop {
        let output = render(&ctx, &mut workspace, vec![]);
        let tab = &workspace.tabs[workspace.active];
        assert!(
            tab.process.is_none(),
            "Embedded Browser requires an in-tab engine."
        );
        let browser = tab.web_browser.as_ref().expect(&tab.error);
        let state = browser.handle.state();
        assert!(!state.closed, "{:?}", state.error);
        if let Some(tex_id) = browser.texture.as_ref().map(TextureHandle::id)
            && output.images.iter().any(|(id, data)| {
                let egui::ImageData::Color(image) = data;
                let pixel = image.pixels[image.pixels.len() / 2];
                *id == tex_id && pixel.r() > 210 && pixel.b() > 210 && pixel.g() < 40
            })
            && let Some(bounds) = ctx
                .tessellate(output.output.shapes, output.output.pixels_per_point)
                .into_iter()
                .find_map(|primitive| match primitive.primitive {
                    egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == tex_id => {
                        Some(mesh.calc_bounds())
                    }
                    _ => None,
                })
        {
            break (bounds, browser.frame_size);
        }
        assert!(
            Instant::now() < deadline,
            "No Chromium frame painted in the workspace."
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let click = |workspace: &mut Workspace, point: [f32; 2]| {
        let pos = canvas.min
            + vec2(
                point[0] * canvas.width() / width as f32,
                point[1] * canvas.height() / height as f32,
            );
        for pressed in [true, false] {
            let _ = render(
                &ctx,
                workspace,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    };
    click(&mut workspace, [40., 25.]);
    let _ = render(&ctx, &mut workspace, vec![Event::Text("from egui".into())]);
    click(&mut workspace, [240., 25.]);
    while !fixture.saw("/accepted?text=from+egui") {
        let _ = render(&ctx, &mut workspace, vec![]);
        assert!(
            Instant::now() < deadline,
            "Browser did not submit the routed egui input."
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let active = workspace.active;
    workspace.force_close_tab(active);
    assert!(workspace.tabs.iter().all(|tab| tab.id != id));
}

fn fixture_runtime() -> harbor_workspace::browser::Runtime {
    if std::env::var_os("TERMCRAFT_TEST_BUNDLED_BROWSER").is_some() {
        harbor_workspace::browser::Runtime::BundledChromium
    } else {
        harbor_workspace::browser::Runtime::SystemChromium
    }
}

fn fixture_budget(normal: u64) -> Duration {
    // Unoptimized SHA verification of the full payload precedes browser startup.
    Duration::from_secs(
        if std::env::var_os("TERMCRAFT_TEST_BUNDLED_BROWSER").is_some() {
            60
        } else {
            normal
        },
    )
}

fn chromium_fixture_slot() -> Option<std::sync::MutexGuard<'static, ()>> {
    if fixture_runtime() == harbor_workspace::browser::Runtime::BundledChromium {
        harbor_workspace::runtime::RuntimeCatalog::for_current_executable()
            .unwrap()
            .resolve(harbor_workspace::runtime::ToolId::BundledChromium)
            .unwrap();
        return Some(owned_process_fixture_slot());
    }
    #[cfg(target_os = "linux")]
    let installed = std::path::Path::new("/usr/lib/chromium/chromium").is_file();
    #[cfg(target_os = "macos")]
    let installed = harbor_workspace::runtime::RuntimeCatalog::for_current_executable()
        .and_then(|catalog| catalog.resolve(harbor_workspace::runtime::ToolId::Browser))
        .is_ok();
    assert!(installed || std::env::var_os("TERMCRAFT_REQUIRE_CHROMIUM").is_none());
    installed.then(owned_process_fixture_slot)
}

fn test_tab(workspace: &mut Workspace, address: String) -> &mut TerminalTab {
    workspace.new_session("Browser");
    let mut profile = workspace.session_form.take().unwrap();
    profile.browser_options.runtime = fixture_runtime();
    profile.host = address;
    workspace.start(profile);
    &mut workspace.tabs[workspace.active]
}

fn render_browser(
    ctx: &Context,
    tab: &mut TerminalTab,
    events: Vec<Event>,
    focus_address: Option<bool>,
) {
    render_browser_at(ctx, tab, vec2(720., 540.), events, focus_address);
}

fn render_browser_at(
    ctx: &Context,
    tab: &mut TerminalTab,
    size: Vec2,
    events: Vec<Event>,
    focus_address: Option<bool>,
) {
    browser_output(ctx, tab, size, events, focus_address)
        .textures_delta
        .clear();
}

fn browser_output(
    ctx: &Context,
    tab: &mut TerminalTab,
    size: Vec2,
    events: Vec<Event>,
    focus_address: Option<bool>,
) -> egui::FullOutput {
    ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| {
            let address = ui.make_persistent_id(("website-address", tab.id));
            if let Some(focused) = focus_address {
                ui.memory_mut(|memory| {
                    if focused {
                        memory.request_focus(address);
                    } else {
                        memory.surrender_focus(address);
                    }
                });
            }
            toolbar(ui, tab);
            view(ui, tab);
        },
    )
}

fn wait_for_browser(mut condition: impl FnMut() -> bool, message: &str) {
    let deadline = Instant::now() + fixture_budget(15);
    while !condition() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn narrow_browser_wraps_address_and_go_without_losing_input_focus() {
    let Some(_slot) = chromium_fixture_slot() else {
        return;
    };
    let fixture = fixture::Fixture::http(|_| fixture::Response::html("<title>Owned page</title>"));
    let (ctx, _root, mut w) = owned_workspace();
    ctx.enable_accesskit();
    let tab = test_tab(&mut w, fixture.origin.clone());
    let size = vec2(300., 380.);
    render_browser_at(&ctx, tab, size, vec![], Some(true));
    let mut out = browser_output(&ctx, tab, size, vec![], Some(true));
    out.textures_delta.clear();
    let back = crate::ui::gui_tests::shell_control_position(&out, "Back");
    let go = crate::ui::gui_tests::shell_text_position(&out, "Go");
    assert!(go.x < size.x && (go.y - back.y).abs() < 8.);
    render_browser_at(&ctx, tab, size, vec![Event::Text("/narrow".into())], None);
    assert!(
        tab.web_browser
            .as_ref()
            .unwrap()
            .address
            .contains("/narrow")
    );
}

#[test]
fn embedded_browser_address_reconciles_redirect_after_editing_blurs() {
    let Some(_chromium_slot) = chromium_fixture_slot() else {
        return;
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
    };

    let destination =
        fixture::Fixture::http(|_| fixture::Response::html("<title>Redirected</title>"));
    let first = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let original = format!("http://{}/first", first.local_addr().unwrap());
    let redirected = format!("{}/committed", destination.origin);
    let response_address = redirected.clone();
    let (received, started) = mpsc::channel();
    let (proceed, release) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        // Chromium can abandon a speculative connection before navigation.
        let mut stream = loop {
            let (mut stream, _) = first.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(750)))
                .unwrap();
            let mut request = [0; 4096];
            if let Ok(count) = stream.read(&mut request)
                && request[..count].starts_with(b"GET /first ")
            {
                break stream;
            }
        };
        received.send(()).unwrap();
        release.recv_timeout(Duration::from_secs(8)).unwrap();
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {response_address}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").unwrap();
    });

    let (ctx, _root, mut workspace) = owned_workspace();
    let tab = test_tab(&mut workspace, original.clone());
    started.recv_timeout(fixture_budget(5)).unwrap();
    render_browser(&ctx, tab, vec![], Some(true));
    render_browser(&ctx, tab, vec![Event::Text("/unfinished".into())], None);
    let draft = tab.web_browser.as_ref().unwrap().address.clone();
    assert_ne!(draft, original);
    proceed.send(()).unwrap();
    wait_for_browser(
        || {
            render_browser(&ctx, tab, vec![], None);
            tab.web_browser.as_ref().unwrap().handle.state().address == redirected
        },
        "Chromium never committed the redirected origin",
    );
    // Let the focused field render after Chromium's commit is observable.
    render_browser(&ctx, tab, vec![], None);
    assert_eq!(tab.web_browser.as_ref().unwrap().address, draft);
    render_browser(&ctx, tab, vec![], Some(false));
    assert_eq!(tab.web_browser.as_ref().unwrap().address, redirected);
    assert!(destination.saw("/committed"));
    worker.join().unwrap();
}

#[test]
fn embedded_browser_delivers_outside_release_when_input_queue_is_full() {
    let Some(_chromium_slot) = chromium_fixture_slot() else {
        return;
    };
    let fixture = fixture::Fixture::http(|_| {
        fixture::Response::html(
            "<title>Pointer fixture</title><body style='margin:0;height:100vh;background:red'>         <script>         let downs = 0, ups = 0;         document.addEventListener('pointerdown', e => {           ++downs; document.body.setPointerCapture(e.pointerId); fetch('/pressed');         });         document.addEventListener('pointerup', () => {++ups;});         document.addEventListener('pointermove', e => {           if(e.clientX > 450) fetch('/move?buttons='+e.buttons+'&downs='+downs+'&ups='+ups);         });         </script>",
        )
    });
    let (ctx, _root, mut workspace) = owned_workspace();
    let tab = test_tab(&mut workspace, fixture.origin.clone());
    let deadline = Instant::now() + fixture_budget(15);
    loop {
        let mut output = browser_output(&ctx, tab, vec2(720., 540.), vec![], None);
        let Some(browser) = tab.web_browser.as_ref() else {
            output.textures_delta.clear();
            panic!("Embedded browser did not start: {}", tab.error);
        };
        let state = browser.handle.state();
        let expected_title = state.title == "Pointer fixture";
        let committed_frame = browser
            .painted
            .as_ref()
            .is_some_and(|paint| paint.revision == browser.viewport_revision);
        let painted_fixture = browser.texture.as_ref().is_some_and(|texture| {
            output.textures_delta.set.iter().any(|(id, deltas)| {
                *id == texture.id()
                    && deltas.iter().any(|delta| {
                        let egui::ImageData::Color(image) = &delta.image;
                        let pixel = image.pixels[image.pixels.len() / 2];
                        pixel.r() > 210 && pixel.g() < 40 && pixel.b() < 40
                    })
            })
        });
        output.textures_delta.clear();
        if committed_frame && painted_fixture && expected_title {
            break;
        }
        assert!(
            !state.closed && Instant::now() < deadline,
            "The pointer fixture never rendered: page_requested={}, expected_title={}, loading={}, closed={}, authenticated_dialog={}, viewport={:?}, viewport_revision={}, texture_revision={:?}, error={:?}, notice={:?}",
            fixture.saw("/"),
            expected_title,
            state.loading,
            state.closed,
            state.challenge.is_some(),
            browser.viewport,
            browser.viewport_revision,
            browser.texture_revision,
            state.error,
            state.notice,
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // Begin the gesture within the canvas, then put a burst of motion and a
    // release outside the canvas into a single UI update.
    let start = pos2(120., 180.);
    render_browser(
        &ctx,
        tab,
        vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        None,
    );
    wait_for_browser(
        || fixture.saw("/pressed"),
        "Chromium never received the press",
    );
    let mut burst: Vec<_> = (0..20_000)
        .map(|index| Event::PointerMoved(pos2(130. + (index % 150) as f32, 190.)))
        .collect();
    burst.push(Event::PointerButton {
        pos: pos2(725., 200.),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    render_browser(&ctx, tab, burst, None);
    assert!(
        tab.web_browser.as_ref().unwrap().input_error.is_some(),
        "Input burst did not saturate the queue"
    );

    wait_for_browser(
        || {
            render_browser(&ctx, tab, vec![Event::PointerMoved(pos2(500., 250.))], None);
            fixture.saw("/move?buttons=0&downs=1&ups=1")
        },
        "Chromium did not receive exactly one release before the next move",
    );
    assert!(!fixture.saw("/move?buttons=1&downs=1&ups=0"));
}

#[test]
fn embedded_browser_blocks_clicks_on_a_resizing_canvas_until_a_new_frame_is_painted() {
    let Some(_chromium_slot) = chromium_fixture_slot() else {
        return;
    };
    let fixture = fixture::Fixture::http(|_| {
        fixture::Response::html(
            r#"<!doctype html><title>Resize fixture</title>
        <style>
          body { margin: 0; }
          button { position:absolute; top:0; height:150px; border:0; }
          #left { left:0; width:250px; background:red; }
          #right { left:250px; right:0; background:blue; }
        </style>
        <button id="left" onpointerdown="report('down-left', event)" onclick="report('left', event)">Left</button>
        <button id="right" onclick="report('right', event)"
          onpointerdown="this.setPointerCapture(event.pointerId); send('/held')"
          ongotpointercapture="send('/captured')"
          onlostpointercapture="send('/lost?buttons=' + event.buttons)"
          onpointerup="send('/released?buttons=' + event.buttons)">Right</button>
        <script>
          function send(path) {
            fetch(path, {cache: "no-store"});
          }
          function report(side, event) {
            send("/click?side=" + side + "&x=" + event.clientX + "&width=" + innerWidth);
          }
          addEventListener("resize", () => send("/size?width=" + innerWidth));
        </script>"#,
        )
    });
    let (ctx, _root, mut workspace) = owned_workspace();
    let tab = test_tab(&mut workspace, fixture.origin.clone());
    let small = vec2(300., 200.);
    let large = vec2(460., 320.);
    wait_for_browser(
        || {
            render_browser_at(&ctx, tab, small, vec![], None);
            let browser = tab.web_browser.as_ref().unwrap();
            let state = browser.handle.state();
            assert!(!state.closed, "{:?}", state.error);
            browser.texture.is_some()
                && browser.frame_size == browser.viewport
                && state.title == "Resize fixture"
        },
        "The small Chromium viewport did not paint",
    );
    let small_width = tab.web_browser.as_ref().unwrap().viewport[0];
    // The unified toolbar leaves a pixel-aligned canvas origin. Hit DOM pixel
    // 289 directly; keep the exact coordinate, viewport and capture assertions.
    let click = || {
        let pos = pos2(289., 115.);
        [true, false]
            .into_iter()
            .map(|pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            })
            .collect()
    };
    let pos = pos2(289., 115.);
    render_browser_at(&ctx, tab, small, vec![Event::PointerMoved(pos)], None);
    render_browser_at(
        &ctx,
        tab,
        large,
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        None,
    );
    render_browser_at(
        &ctx,
        tab,
        large,
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        None,
    );
    let width = tab.web_browser.as_ref().unwrap().viewport[0];
    wait_for_browser(
        || fixture.saw(&format!("/size?width={width}")),
        "The renderer did not resize",
    );
    wait_for_browser(
        || {
            render_browser_at(&ctx, tab, large, vec![], None);
            let browser = tab.web_browser.as_ref().unwrap();
            browser.frame_size == browser.viewport
        },
        "The resized Chromium viewport did not paint",
    );
    render_browser_at(&ctx, tab, large, click(), None);
    wait_for_browser(
        || {
            fixture
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|request| request.target.starts_with("/click?side=right&"))
        },
        "The click after the new frame did not hit the right control",
    );
    let count = |prefix: &str| {
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.target.starts_with(prefix))
            .count()
    };
    assert_eq!(count("/click?"), 1);
    assert!(
        fixture.saw(&format!("/click?side=right&x=289&width={width}")),
        "Unexpected click coordinates: {:?}",
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.target.clone())
            .collect::<Vec<_>>()
    );

    // Leave the large frame on screen while the renderer makes a round trip
    // through the small viewport. The old large frame has the same dimensions
    // as the final request, but a different receipt.
    let old_revision = tab.web_browser.as_ref().unwrap().viewport_revision;
    render_browser_at(
        &ctx,
        tab,
        large,
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        None,
    );
    wait_for_browser(
        || count("/held") == 2,
        "The second press did not hold a button",
    );
    render_browser_at(
        &ctx,
        tab,
        large,
        vec![Event::PointerMoved(pos + vec2(1., 0.))],
        None,
    );
    wait_for_browser(
        || count("/captured") == 2,
        "Chromium did not activate capture",
    );
    let small_resize = format!("/size?width={small_width}");
    let small_resizes = count(&small_resize);
    render_browser_at(&ctx, tab, small, vec![], None);
    wait_for_browser(
        || count("/lost?buttons=0") == 2 && count(&small_resize) > small_resizes,
        "The resize did not clear Chromium capture with zero buttons",
    );
    render_browser_at(&ctx, tab, large, click(), None);
    let browser = tab.web_browser.as_ref().unwrap();
    let revision = browser.viewport_revision;
    assert_eq!(revision, old_revision + 2);
    assert_ne!(browser.texture_revision, Some(revision));

    wait_for_browser(
        || {
            render_browser_at(&ctx, tab, large, vec![], None);
            let browser = tab.web_browser.as_ref().unwrap();
            browser
                .painted
                .as_ref()
                .is_some_and(|painted| painted.revision == revision)
        },
        "The final viewport receipt was not painted",
    );
    render_browser_at(&ctx, tab, large, click(), None);
    wait_for_browser(
        || {
            count("/click?side=right&") >= 2
                && count("/held") >= 3
                && count("/released?buttons=0") >= 2
        },
        "The final click did not reach the intended control after the resize round trip",
    );
    assert_eq!(
        count("/click?"),
        2,
        "Stale input or a canceled gesture activated a control"
    );
    assert_eq!(count("/held"), 3);
    assert_eq!(count("/released?buttons=0"), 2);
}

#[test]
fn unified_bar_can_close_the_last_embedded_browser_without_reusing_its_tab() {
    let Some(_slot) = chromium_fixture_slot() else {
        return;
    };
    let fixture =
        fixture::Fixture::http(|_| fixture::Response::html("<title>Owned close fixture</title>"));
    let (ctx, _root, mut w) = owned_workspace();
    ctx.enable_accesskit();
    test_tab(&mut w, fixture.origin.clone());
    w.store.preferences.confirm_session_close = false;
    render(&ctx, &mut w, vec![]);
    let out = render(&ctx, &mut w, vec![]);
    let bounds = out
        .output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, n)| {
            n.label()
                .is_some_and(|s| s.starts_with("Close "))
                .then(|| n.bounds())
                .flatten()
        })
        .expect("Browser close button");
    let p = pos2(
        ((bounds.x0 + bounds.x1) / 2.) as f32,
        ((bounds.y0 + bounds.y1) / 2.) as f32,
    );
    for pressed in [true, false] {
        render(
            &ctx,
            &mut w,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert!(w.tabs.is_empty());
    render(&ctx, &mut w, vec![]);
}
