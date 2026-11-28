//! Original TermCraft glyphs: familiar remote-workspace metaphors on a 24-unit grid.
//! These are drawn locally; no MobaXterm image assets are bundled.
use super::*;

pub(super) fn paint(ui: &Ui, rect: Rect, kind: &str, foreground: Color32) {
    let p = ui.painter();
    let q = |x: f32, y: f32| rect.min + vec2(x * rect.width() / 24., y * rect.height() / 24.);
    let accent = if ui.is_enabled() {
        fluent::palette(ui).accent
    } else {
        foreground
    };
    let stroke = Stroke::new(1.5, foreground);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([q(a.0, a.1), q(b.0, b.1)], stroke);
    };
    let outline = |a: (f32, f32), b: (f32, f32)| {
        p.rect(
            qrect(q, a, b),
            3.,
            accent.gamma_multiply(0.10),
            stroke,
            StrokeKind::Inside,
        );
    };
    let badge = |symbol: &str| {
        let r = Rect::from_min_max(q(12., 12.), q(24., 24.));
        p.rect_filled(r, 3., fluent::palette(ui).surface);
        let b = |x: f32, y: f32| r.min + vec2(x * r.width() / 12., y * r.height() / 12.);
        let pen = Stroke::new(1.5, accent);
        let segment = |a: (f32, f32), z: (f32, f32)| {
            p.line_segment([b(a.0, a.1), b(z.0, z.1)], pen);
        };
        match symbol {
            "+" => {
                segment((2., 6.), (10., 6.));
                segment((6., 2.), (6., 10.));
            }
            "⌑" => {
                p.circle_stroke(b(6., 4.), r.width() * 0.22, pen);
                p.rect_filled(Rect::from_min_max(b(2., 5.), b(10., 11.)), 1., accent);
            }
            "⇄" => {
                segment((1., 3.), (10., 3.));
                segment((7., 1.), (10., 3.));
                segment((7., 5.), (10., 3.));
                segment((2., 9.), (11., 9.));
                segment((2., 9.), (5., 7.));
                segment((2., 9.), (5., 11.));
            }
            "↧" => {
                segment((6., 1.), (6., 9.));
                segment((2., 6.), (6., 10.));
                segment((10., 6.), (6., 10.));
            }
            "↗" => {
                segment((2., 10.), (10., 2.));
                segment((3., 2.), (10., 2.));
                segment((10., 2.), (10., 9.));
            }
            "◉" => {
                p.circle_stroke(b(6., 6.), r.width() * 0.35, pen);
                p.circle_filled(b(6., 6.), r.width() * 0.15, accent);
            }
            "X" => {
                segment((2., 2.), (10., 10.));
                segment((10., 2.), (2., 10.));
            }
            "~" => {
                p.add(PathShape::line(
                    vec![b(1., 7.), b(3., 4.), b(8., 8.), b(11., 5.)],
                    pen,
                ));
            }
            _ => {
                segment((3., 2.), (9., 6.));
                segment((9., 6.), (3., 10.));
            }
        }
    };
    match kind {
        "session" | "ssh" | "terminal" | "mosh" => {
            outline((2., 3.), (21., 18.));
            line((5., 7.), (9., 10.));
            line((9., 10.), (5., 13.));
            line((12., 13.), (17., 13.));
            badge(match kind {
                "session" => "+",
                "ssh" => "⌑",
                "mosh" => "~",
                _ => "›",
            });
        }
        "servers" => {
            for y in [3., 10., 17.] {
                outline((3., y), (21., y + 5.));
                p.circle_filled(q(6., y + 2.5), rect.width() / 30., accent);
            }
        }
        "sessions" | "folder" | "sftp" | "ftp" | "packages" | "new-folder" => {
            p.add(PathShape::closed_line(
                vec![
                    q(2., 5.),
                    q(9., 5.),
                    q(12., 8.),
                    q(22., 8.),
                    q(22., 21.),
                    q(2., 21.),
                ],
                stroke,
            ));
            p.rect_filled(
                Rect::from_min_max(q(4., 10.), q(20., 19.)),
                1.,
                accent.gamma_multiply(0.15),
            );
            match kind {
                "sftp" => badge("⇄"),
                "ftp" => badge("↧"),
                "packages" => {
                    line((7., 12.), (17., 12.));
                    line((12., 10.), (12., 18.));
                }
                "new-folder" => badge("+"),
                _ => {}
            }
        }
        "rdp" | "vnc" | "desktop" | "xserver" => {
            outline((2., 3.), (22., 17.));
            line((12., 17.), (12., 21.));
            line((7., 21.), (17., 21.));
            match kind {
                "rdp" => badge("↗"),
                "vnc" => badge("◉"),
                "xserver" => badge("X"),
                _ => {}
            }
        }
        "split" => {
            outline((2., 3.), (22., 21.));
            line((12., 3.), (12., 21.));
            p.rect_filled(
                Rect::from_min_max(q(4., 5.), q(10., 19.)),
                1.,
                accent.gamma_multiply(0.20),
            );
        }
        "broadcast" => {
            outline((1., 3.), (10., 12.));
            outline((14., 3.), (23., 12.));
            line((5., 16.), (19., 16.));
            line((12., 16.), (12., 22.));
        }
        "tunnel" | "link" => {
            outline((1., 7.), (8., 17.));
            outline((16., 7.), (23., 17.));
            line((8., 12.), (16., 12.));
            line((11., 9.), (14., 12.));
            line((14., 12.), (11., 15.));
        }
        "menu" => {
            for y in [6., 12., 18.] {
                line((4., y), (20., y));
            }
        }
        "theme" => {
            p.circle_stroke(q(12., 12.), rect.width() * 0.36, stroke);
            p.add(PathShape::convex_polygon(
                vec![
                    q(12., 4.),
                    q(18., 6.),
                    q(21., 12.),
                    q(18., 18.),
                    q(12., 20.),
                ],
                accent,
                Stroke::NONE,
            ));
        }
        "search" => {
            p.circle_stroke(q(10., 10.), rect.width() * 0.29, stroke);
            line((15., 15.), (22., 22.));
        }
        "refresh" | "connect" => {
            p.circle_stroke(q(12., 12.), rect.width() * 0.34, stroke);
            p.rect_filled(
                Rect::from_min_max(q(13., 1.), q(23., 10.)),
                0.,
                fluent::palette(ui).surface,
            );
            line((18., 3.), (20., 9.));
            line((20., 9.), (14., 9.));
            if kind == "connect" {
                badge("+");
            }
        }
        "upload" | "download" => {
            let (d, t) = if kind == "upload" {
                (5., 17.)
            } else {
                (17., 5.)
            };
            line((12., t), (12., d));
            line((7., if kind == "upload" { 10. } else { 12. }), (12., d));
            line((17., if kind == "upload" { 10. } else { 12. }), (12., d));
            line((4., 20.), (20., 20.));
            line((4., 17.), (4., 20.));
            line((20., 17.), (20., 20.));
        }
        "copy" => {
            outline((7., 7.), (21., 22.));
            p.add(PathShape::line(
                vec![q(4., 17.), q(2., 17.), q(2., 2.), q(16., 2.), q(16., 4.)],
                stroke,
            ));
        }
        "delete" => {
            line((3., 6.), (21., 6.));
            line((8., 3.), (16., 3.));
            p.add(PathShape::line(
                vec![q(5., 8.), q(6., 22.), q(18., 22.), q(19., 8.)],
                stroke,
            ));
            line((10., 10.), (10., 19.));
            line((14., 10.), (14., 19.));
        }
        "save" => {
            outline((3., 2.), (21., 22.));
            outline((7., 3.), (17., 9.));
            outline((7., 14.), (17., 21.));
        }
        "rename" => {
            line((3., 21.), (8., 20.));
            line((3., 21.), (4., 16.));
            p.add(PathShape::closed_line(
                vec![q(4., 16.), q(17., 3.), q(21., 7.), q(8., 20.)],
                stroke,
            ));
        }
        "back" | "forward" => {
            let (x, y) = if kind == "back" { (5., 19.) } else { (19., 5.) };
            line((x, 12.), (y, 12.));
            line((x, 12.), (if kind == "back" { 11. } else { 13. }, 6.));
            line((x, 12.), (if kind == "back" { 11. } else { 13. }, 18.));
        }
        "browser" => {
            outline((2., 3.), (22., 21.));
            line((2., 8.), (22., 8.));
            p.circle_filled(q(5., 5.5), rect.width() / 35., accent);
            p.circle_stroke(q(12., 14.), rect.width() * 0.20, stroke);
            line((7., 14.), (17., 14.));
        }
        "serial" => {
            outline((2., 6.), (22., 17.));
            for x in [6., 10., 14., 18.] {
                p.circle_filled(q(x, 10.), rect.width() / 30., accent);
            }
            line((8., 17.), (8., 22.));
            line((16., 17.), (16., 22.));
        }
        "macro" => {
            outline((3., 3.), (17., 21.));
            line((6., 7.), (13., 7.));
            line((6., 11.), (13., 11.));
            p.add(PathShape::convex_polygon(
                vec![q(15., 13.), q(23., 17.), q(15., 22.)],
                accent,
                Stroke::NONE,
            ));
        }
        "disconnect" => {
            outline((6., 6.), (18., 18.));
        }
        "home" => {
            p.add(PathShape::line(
                vec![q(2., 11.), q(12., 2.), q(22., 11.)],
                stroke,
            ));
            p.add(PathShape::line(
                vec![q(5., 10.), q(5., 22.), q(19., 22.), q(19., 10.)],
                stroke,
            ));
        }
        _ => super::draw_icon(p, rect, kind, foreground),
    }
}
fn qrect(q: impl Fn(f32, f32) -> Pos2, a: (f32, f32), b: (f32, f32)) -> Rect {
    Rect::from_min_max(q(a.0, a.1), q(b.0, b.1))
}

pub(super) fn button(ui: &mut Ui, kind: &str, label: &str, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(32., 32.), Sense::click());
    let color = fluent::paint_control(ui, &response, rect, selected, ControlKind::Flat);
    paint(
        ui,
        Rect::from_center_size(rect.center(), vec2(21., 21.)),
        kind,
        color,
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, response.enabled(), label));
    response.fluent_hint(label)
}
pub(super) fn menu(
    ui: &mut Ui,
    kind: &str,
    label: &str,
    content: impl FnOnce(&mut Ui),
) -> Response {
    let response = ui
        .push_id(label, |ui| ui.menu_button("    ", content).response)
        .inner;
    paint(
        ui,
        Rect::from_center_size(response.rect.center(), vec2(20., 20.)),
        kind,
        ink(ui),
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, response.enabled(), label));
    response.fluent_hint(label)
}
pub(super) fn action(ui: &mut Ui, label: &str) -> Response {
    let text = ui
        .painter()
        .layout_no_wrap(label.into(), FontId::proportional(14.), ink(ui));
    let (rect, response) = ui.allocate_exact_size(vec2(text.size().x + 42., 32.), Sense::click());
    let color = fluent::paint_control(ui, &response, rect, false, ControlKind::Flat);
    paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 14., rect.center().y), vec2(18., 18.)),
        semantic(label),
        color,
    );
    ui.painter().galley(
        pos2(rect.left() + 30., rect.center().y - text.size().y / 2.),
        text,
        color,
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, response.enabled(), label));
    response.fluent_hint(label)
}
pub(super) fn semantic(label: &str) -> &'static str {
    let l = label.to_ascii_lowercase();
    for (prefix, kind) in [
        ("upload", "upload"),
        ("download", "download"),
        ("new folder", "new-folder"),
        ("delete", "delete"),
        ("remove", "delete"),
        ("rename", "rename"),
        ("edit", "rename"),
        ("refresh", "refresh"),
        ("reload", "refresh"),
        ("save", "save"),
        ("copy", "copy"),
        ("paste", "copy"),
        ("find", "search"),
        ("search", "search"),
        ("ssh", "ssh"),
        ("credential", "lock"),
        ("new session", "session"),
        ("start local", "terminal"),
        ("import", "download"),
        ("export", "upload"),
        ("local files", "folder"),
        ("shared session", "sessions"),
        ("keyboard", "settings"),
        ("home", "home"),
        ("stop", "disconnect"),
        ("cancel", "disconnect"),
        ("connect", "connect"),
        ("start", "connect"),
        ("servers", "servers"),
        ("tools", "tools"),
        ("sessions", "sessions"),
        ("saved sessions", "sessions"),
        ("games", "games"),
        ("settings", "settings"),
        ("configuration", "settings"),
        ("manage macros", "macro"),
        ("x server", "xserver"),
        ("exit", "exit"),
        ("help", "help"),
        ("feature parity", "help"),
        ("choose", "folder"),
        ("browse", "folder"),
        ("back", "back"),
        ("forward", "forward"),
    ] {
        if l.starts_with(prefix) {
            return kind;
        }
    }
    "settings"
}

pub(super) fn tile(
    ui: &mut Ui,
    kind: &str,
    label: &str,
    hint: &str,
    selected: Option<bool>,
) -> Response {
    let font = FontId::proportional(12.);
    let width = (ui
        .painter()
        .layout_no_wrap(label.into(), font.clone(), ink(ui))
        .size()
        .x
        + 16.)
        .max(56.);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 56.), Sense::click());
    let foreground = fluent::paint_control(
        ui,
        &response,
        rect,
        selected.unwrap_or(false),
        ControlKind::Flat,
    );
    let foreground = if response.enabled() {
        foreground
    } else {
        fluent::palette(ui).secondary_text
    };
    if let Some(selected) = selected {
        response.widget_info(|| {
            WidgetInfo::selected(WidgetType::Checkbox, response.enabled(), selected, hint)
        });
        if selected && response.enabled() {
            let mark = Rect::from_center_size(rect.center_bottom() - vec2(0., 3.), vec2(18., 2.));
            ui.painter().rect_filled(mark, 1., accent(ui));
        }
    } else {
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, response.enabled(), hint));
    }
    paint(
        ui,
        Rect::from_center_size(rect.center_top() + vec2(0., 18.), vec2(23., 23.)),
        match label {
            "Session" => "session",
            "Servers" => "servers",
            "Sessions" => "sessions",
            "Split" => "split",
            "MultiExec" => "broadcast",
            "Tunneling" => "tunnel",
            "Packages" => "packages",
            "X server" => "xserver",
            _ => kind,
        },
        foreground,
    );
    ui.painter().text(
        rect.center_bottom() - vec2(0., 13.),
        Align2::CENTER_CENTER,
        label,
        font,
        foreground,
    );
    if response.enabled() {
        response.fluent_hint(hint)
    } else {
        response.disabled_hint(hint)
    }
}
