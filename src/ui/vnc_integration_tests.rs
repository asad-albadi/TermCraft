use super::{terminal_review_tests::owned_workspace, *};
use harbor_workspace::desktop::rdp_options::ClipboardPolicy;

#[allow(dead_code)]
#[path = "../../tests/support/vnc_server.rs"]
mod fixture;

struct Rendered {
    output: egui::FullOutput,
    images: Vec<(egui::TextureId, egui::ImageData)>,
}

fn render(ctx: &Context, workspace: &mut Workspace, events: Vec<Event>) -> Rendered {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 900.))),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| {
            eframe::App::logic(workspace, ui.ctx(), &mut eframe::Frame::_new_kittest());
            workspace.render(ui);
        },
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

fn texture(rendered: &Rendered, size: [usize; 2]) -> (egui::TextureId, &[Color32]) {
    rendered
        .images
        .iter()
        .find_map(|(id, data)| {
            let egui::ImageData::Color(image) = data;
            (image.size == size).then_some((*id, image.pixels.as_slice()))
        })
        .expect("remote image must be uploaded to the renderer")
}

fn painted_bounds(ctx: &Context, output: egui::FullOutput, texture: egui::TextureId) -> Rect {
    ctx.tessellate(output.shapes, output.pixels_per_point)
        .into_iter()
        .find_map(|primitive| match primitive.primitive {
            egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture => {
                Some(mesh.calc_bounds())
            }
            _ => None,
        })
        .expect("remote texture must be painted in the workspace")
}

#[test]
fn embedded_vnc_workspace_paints_real_frame_and_cursor_and_routes_scaled_input() {
    let (ctx, _root, mut workspace) = owned_workspace();
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    workspace.start(config.session);
    let tab_index = workspace.active;
    let tab = &workspace.tabs[tab_index];
    let canvas_id = Id::new(("rdp-canvas", tab.id));
    assert!(tab.desktop.is_some(), "{}", tab.error);
    assert!(tab.process.is_none() && tab.launch.is_none());

    let mut server = fixture::accept(listener);
    assert!(fixture::encodings(&mut server).contains(&-239));
    fixture::read(&mut server, 10);
    let mut fb = vec![0; 20 * 4 * 4];
    fb[84..88].copy_from_slice(&[0, 255, 0, 0]); // Pixel (1, 1), in RFB BGRX order.
    fixture::update(&mut server, (0, 0, 20, 4), 0, &fb);
    fixture::read(&mut server, 10);
    fixture::update(
        &mut server,
        (1, 0, 2, 1),
        -239,
        &[255, 0, 0, 0, 0, 0, 255, 0, 0x80], // Visible blue, masked-out red.
    );
    fixture::read(&mut server, 10);

    let output = render(&ctx, &mut workspace, vec![]);
    let desktop = workspace.tabs[tab_index].desktop.as_ref().unwrap();
    assert!(desktop.connected);
    let canvas = desktop.canvas_rect.expect("visible embedded desktop");
    let frame = desktop.frame.as_ref().unwrap();
    assert_eq!((frame.width, frame.height), (20, 4));
    let (frame_texture, pixels) = texture(&output, [20, 4]);
    assert_eq!(pixels[21], Color32::GREEN);
    assert_eq!(pixels[20], Color32::BLACK);
    let frame_bounds = painted_bounds(&ctx, output.output, frame_texture);
    assert!((frame_bounds.min - canvas.min).length() < 0.6);
    assert!((frame_bounds.max - canvas.max).length() < 0.6);

    let scale = canvas.width() / 20.;
    assert!(scale > 1.);
    let target = canvas.min + vec2(7.5, 2.5) * scale;
    let output = render(&ctx, &mut workspace, vec![Event::PointerMoved(target)]);
    assert_eq!(output.output.platform_output.cursor_icon, CursorIcon::None);
    let (cursor_texture, pixels) = texture(&output, [2, 1]);
    assert_eq!(pixels, [Color32::BLUE, Color32::TRANSPARENT]);
    let cursor_bounds = painted_bounds(&ctx, output.output, cursor_texture);
    assert!((cursor_bounds.min - (target - vec2(scale, 0.))).length() < 0.6);
    assert!((cursor_bounds.size() - vec2(2. * scale, scale)).length() < 0.6);
    assert_eq!(fixture::read(&mut server, 6), [5, 0, 0, 7, 0, 2]);

    for pressed in [true, false] {
        render(
            &ctx,
            &mut workspace,
            vec![Event::PointerButton {
                pos: target,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(
            fixture::read(&mut server, 6),
            [5, u8::from(pressed), 0, 7, 0, 2]
        );
    }
    assert!(ctx.memory(|memory| memory.has_focus(canvas_id)));
    render(
        &ctx,
        &mut workspace,
        [true, false]
            .into_iter()
            .map(|pressed| Event::Key {
                key: Key::ArrowLeft,
                physical_key: Some(Key::ArrowLeft),
                pressed,
                modifiers: Modifiers::NONE,
                repeat: false,
            })
            .collect(),
    );
    assert_eq!(
        fixture::read(&mut server, 16),
        [4, 1, 0, 0, 0, 0, 0xff, 0x51, 4, 0, 0, 0, 0, 0, 0xff, 0x51]
    );
}

#[test]
fn timed_macro_records_vnc_gui_input_and_releases_held_buttons_on_cancel() {
    use harbor_workspace::macros::{Action, Event as Step, Script};

    let (ctx, _root, mut workspace) = owned_workspace();
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    workspace.start(config.session);
    let id = workspace.tabs[0].id;
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    fixture::update(&mut server, (0, 0, 20, 4), 0, &[0; 20 * 4 * 4]);
    fixture::read(&mut server, 10);
    render(&ctx, &mut workspace, vec![]);
    workspace.start_macro_recording(0).unwrap();
    let canvas = workspace.tabs[0]
        .desktop
        .as_ref()
        .unwrap()
        .canvas_rect
        .unwrap();
    let scale = canvas.width() / 20.;
    let point = canvas.min + vec2(7.5, 2.5) * scale;
    render(
        &ctx,
        &mut workspace,
        vec![
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::Key {
                key: Key::ArrowLeft,
                physical_key: Some(Key::ArrowLeft),
                pressed: true,
                modifiers: Modifiers::NONE,
                repeat: false,
            },
        ],
    );
    assert_eq!(fixture::read(&mut server, 6), [5, 1, 0, 7, 0, 2]);
    assert_eq!(
        fixture::read(&mut server, 8),
        [4, 1, 0, 0, 0, 0, 0xff, 0x51]
    );
    workspace.finish_macro_recording(0);
    let recorded = workspace.macro_form.script.take().unwrap();
    assert!(recorded.events.contains(&Step::Input(Action::Key {
        code: 0x14b,
        pressed: true
    })));
    assert!(
        recorded
            .events
            .iter()
            .any(|event| matches!(event, Step::Input(Action::Pointer { buttons: 1, .. })))
    );

    // Return manual focus and held input to neutral before replaying behind another tab.
    render(&ctx, &mut workspace, vec![Event::WindowFocused(false)]);
    assert_eq!(
        fixture::read(&mut server, 8),
        [4, 0, 0, 0, 0, 0, 0xff, 0x51]
    );
    assert_eq!(fixture::read(&mut server, 6), [5, 0, 0, 7, 0, 2]);
    let mut events = recorded.events;
    events.push(Step::Wait { millis: 10_000 });
    let value = Macro {
        name: "desktop".into(),
        script: Some(Script { version: 1, events }),
        ..Macro::default()
    };
    super::terminal_review_tests::raw_terminal(&mut workspace, "other");
    workspace.queue_macro(id, &value).unwrap();
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        workspace.poll_macros(&ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fixture::read(&mut server, 6), [5, 0, 0, 7, 0, 2]);
    assert_eq!(fixture::read(&mut server, 6), [5, 1, 0, 7, 0, 2]);
    assert_eq!(
        fixture::read(&mut server, 8),
        [4, 1, 0, 0, 0, 0, 0xff, 0x51]
    );
    workspace.cancel_macro(id);
    assert_eq!(
        fixture::read(&mut server, 8),
        [4, 0, 0, 0, 0, 0, 0xff, 0x51]
    );
    assert_eq!(fixture::read(&mut server, 6), [5, 0, 0, 7, 0, 2]);
    assert!(super::terminal_review_tests::captured(&workspace.tabs[1]).is_empty());

    let value = Macro {
        name: "completion".into(),
        script: Some(Script {
            version: 1,
            events: vec![Step::Input(Action::Key {
                code: 0x14b,
                pressed: true,
            })],
        }),
        ..Macro::default()
    };
    workspace.queue_macro(id, &value).unwrap();
    workspace.poll_macros(&ctx);
    assert_eq!(
        fixture::read(&mut server, 16),
        [4, 1, 0, 0, 0, 0, 0xff, 0x51, 4, 0, 0, 0, 0, 0, 0xff, 0x51]
    );
}
