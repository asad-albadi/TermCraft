use eframe::egui::{
    self, Color32, Context, CornerRadius, FontId, Frame, Id, Label, Margin, Modal, ModalResponse,
    Rect, Response, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, TextStyle, Theme,
    Ui, Visuals, WidgetInfo, WidgetType, Window, vec2,
};

pub(super) const CONTROL_HEIGHT: f32 = 32.;
pub(super) const PAGE_TITLE: f32 = 28.;
pub(super) const DIALOG_TITLE: f32 = 20.;
const CHROME_MARGIN: i8 = 16;
// Windows 11 rounds in-page controls and floating surfaces at different scales.
pub(super) const CONTROL_RADIUS: CornerRadius = CornerRadius::same(4);
pub(super) const OVERLAY_RADIUS: CornerRadius = CornerRadius::same(8);
const STYLE_SETTINGS: &str = "termcraft-fluent-style-settings";

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub surface: Color32,
    pub floating: Color32,
    pub text: Color32,
    pub secondary_text: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub focus: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub success: Color32,
    pub control: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    primary: Color32,
    primary_hover: Color32,
    primary_pressed: Color32,
    on_primary: Color32,
    high_contrast: bool,
}

impl Palette {
    fn new(dark: bool, high_contrast: bool) -> Self {
        let rgb = Color32::from_rgb;
        let mut palette = if dark {
            Self {
                surface: rgb(19, 25, 23),
                floating: rgb(27, 35, 31),
                text: rgb(234, 242, 237),
                secondary_text: rgb(181, 200, 190),
                border: rgb(101, 123, 113),
                accent: rgb(89, 227, 171),
                focus: rgb(127, 229, 190),
                error: rgb(255, 161, 165),
                warning: rgb(255, 209, 115),
                success: rgb(120, 229, 166),
                control: rgb(32, 44, 38),
                hover: rgb(38, 59, 49),
                pressed: rgb(47, 69, 58),
                selected: rgb(34, 72, 54),
                primary: rgb(72, 214, 156),
                primary_hover: rgb(111, 230, 178),
                primary_pressed: rgb(45, 182, 126),
                on_primary: rgb(6, 34, 24),
                high_contrast,
            }
        } else {
            Self {
                surface: rgb(251, 253, 252),
                floating: rgb(239, 245, 242),
                text: rgb(24, 39, 32),
                secondary_text: rgb(73, 88, 81),
                border: rgb(132, 149, 140),
                accent: rgb(0, 104, 75),
                focus: rgb(0, 105, 76),
                error: rgb(153, 28, 46),
                warning: rgb(119, 73, 0),
                success: rgb(18, 104, 61),
                control: rgb(245, 249, 247),
                hover: rgb(226, 240, 232),
                pressed: rgb(210, 229, 219),
                selected: rgb(209, 234, 221),
                primary: rgb(0, 112, 82),
                primary_hover: rgb(0, 121, 86),
                primary_pressed: rgb(0, 83, 60),
                on_primary: Color32::WHITE,
                high_contrast,
            }
        };
        if high_contrast {
            palette.surface = if dark { Color32::BLACK } else { Color32::WHITE };
            palette.floating = palette.surface;
            palette.control = palette.surface;
            palette.text = if dark { Color32::WHITE } else { Color32::BLACK };
            palette.secondary_text = palette.text;
            palette.border = palette.text;
        }
        palette
    }
}

/// Colors for app-owned chrome. Detached viewports inherit the same context settings.
pub(super) fn palette(ui: &Ui) -> Palette {
    let high_contrast = ui.ctx().data(|data| {
        data.get_temp::<(bool, bool)>(Id::new(STYLE_SETTINGS))
            .is_some_and(|(_, high_contrast)| high_contrast)
    });
    Palette::new(ui.visuals().dark_mode, high_contrast)
}

/// Install both styles so System theme changes never discard accessibility settings.
pub(super) fn setup(ctx: &Context, reduced_motion: bool, high_contrast: bool) {
    let settings = (reduced_motion, high_contrast);
    let id = Id::new(STYLE_SETTINGS);
    if ctx.data(|data| data.get_temp(id)) == Some(settings) {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(id, settings));

    for theme in [Theme::Light, Theme::Dark] {
        let dark = theme == Theme::Dark;
        let p = Palette::new(dark, high_contrast);
        let mut style = (*ctx.style_of(theme)).clone();
        let mut visuals = if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        visuals.override_text_color = Some(p.text);
        visuals.weak_text_color = Some(p.secondary_text);
        visuals.panel_fill = p.surface;
        visuals.window_fill = p.floating;
        visuals.extreme_bg_color = p.surface;
        visuals.faint_bg_color = p.control;
        visuals.window_corner_radius = OVERLAY_RADIUS;
        visuals.menu_corner_radius = OVERLAY_RADIUS;
        visuals.window_stroke = Stroke::new(1., p.border);
        visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 96 } else { 40 }),
        };
        visuals.popup_shadow = visuals.window_shadow;
        visuals.hyperlink_color = p.accent;
        visuals.warn_fg_color = p.warning;
        visuals.error_fg_color = p.error;
        visuals.selection.bg_fill = p.selected;
        visuals.selection.stroke = Stroke::new(1., p.text);
        for (widget, fill, border, width) in [
            (&mut visuals.widgets.noninteractive, p.surface, p.border, 1.),
            (&mut visuals.widgets.inactive, p.control, p.border, 1.),
            (&mut visuals.widgets.hovered, p.hover, p.accent, 1.),
            (&mut visuals.widgets.active, p.pressed, p.focus, 2.),
            (&mut visuals.widgets.open, p.selected, p.accent, 1.),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = Stroke::new(width, border);
            widget.fg_stroke.color = p.text;
            widget.corner_radius = CONTROL_RADIUS;
            widget.expansion = 0.;
        }
        style.visuals = visuals;
        style.spacing.item_spacing = vec2(8., 8.);
        style.spacing.button_padding = vec2(12., 4.);
        style.spacing.interact_size.y = CONTROL_HEIGHT;
        style.spacing.window_margin = Margin::same(CHROME_MARGIN);
        style.spacing.menu_margin = Margin::same(4);
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 8.,
            floating_width: 2.,
            floating_allocated_width: 8.,
            foreground_color: true,
            dormant_background_opacity: 0.,
            active_background_opacity: 0.,
            interact_background_opacity: 0.24,
            interact_handle_opacity: 1.,
            ..egui::style::ScrollStyle::thin()
        };
        style.animation_time = if reduced_motion { 0. } else { 0.12 };
        style.scroll_animation = if reduced_motion {
            egui::style::ScrollAnimation::none()
        } else {
            egui::style::ScrollAnimation::default()
        };
        for (text_style, points) in [
            (TextStyle::Body, 14.),
            (TextStyle::Button, 14.),
            (TextStyle::Small, 12.),
            (TextStyle::Heading, PAGE_TITLE),
        ] {
            style
                .text_styles
                .insert(text_style, FontId::proportional(points));
        }
        ctx.set_style_of(theme, style);
    }
    ctx.request_repaint();
}

#[derive(Clone, Copy)]
pub(super) enum ControlKind {
    Standard,
    Primary,
}

/// Paint a selectable or ordinary control without moving its hit target. Use the
/// returned foreground for its icon and text. The caller owns its sense and label.
pub(super) fn paint_control(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    selected: bool,
    kind: ControlKind,
) -> Color32 {
    let p = palette(ui);
    let primary = matches!(kind, ControlKind::Primary);
    let enabled = response.enabled();
    let pressed = response.is_pointer_button_down_on();
    let hovered = response.hovered();
    let fill = match (enabled, primary, pressed, hovered, selected) {
        (false, ..) => p.control,
        (true, true, true, ..) => p.primary_pressed,
        (true, true, false, true, _) => p.primary_hover,
        (true, true, ..) => p.primary,
        (true, false, true, ..) => p.pressed,
        (true, false, false, true, _) => p.hover,
        (true, false, false, false, true) => p.selected,
        (true, false, false, false, false) => p.control,
    };
    let text = if primary && enabled {
        p.on_primary
    } else {
        p.text
    };
    let border = if primary && enabled {
        fill
    } else if selected && enabled {
        p.accent
    } else {
        p.border
    };
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            CONTROL_RADIUS,
            fill,
            Stroke::new(1., border),
            StrokeKind::Inside,
        );
        if enabled
            && hovered
            && !pressed
            && !p.high_contrast
            && let Some(pos) = response.hover_pos()
        {
            // A small, clipped illumination follows the pointer with no timer.
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(pos, Color32::from_white_alpha(16));
            for i in 0..=16 {
                let angle = std::f32::consts::TAU * i as f32 / 16.;
                mesh.colored_vertex(
                    pos + 18. * vec2(angle.cos(), angle.sin()),
                    Color32::TRANSPARENT,
                );
            }
            for i in 1..=16 {
                mesh.add_triangle(0, i, i + 1);
            }
            painter
                .with_clip_rect(rect.shrink(2.))
                .add(egui::Shape::mesh(mesh));
        }
        if enabled && response.has_focus() {
            painter.rect_stroke(
                rect.shrink(2.),
                CONTROL_RADIUS - 2,
                Stroke::new(2., if primary { text } else { p.focus }),
                StrokeKind::Inside,
            );
        }
    }
    text
}

pub(super) fn primary(ui: &mut Ui, label: &str) -> Response {
    let padding = ui.spacing().button_padding;
    let max_width = (ui.available_width() - 2. * padding.x).max(1.);
    let galley = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        max_width,
        TextStyle::Button,
    );
    let size = (galley.size() + 2. * padding).max(vec2(0., CONTROL_HEIGHT));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, response.enabled(), label));
    let text = paint_control(ui, &response, rect, false, ControlKind::Primary);
    if ui.is_rect_visible(rect) {
        ui.painter().galley_with_override_text_color(
            rect.center() - galley.size() * 0.5,
            galley,
            text,
        );
    }
    response
}

pub(super) fn surface(ui: &Ui) -> Frame {
    let p = palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1., p.border))
        .corner_radius(CONTROL_RADIUS)
        .inner_margin(CHROME_MARGIN)
}

#[derive(Clone, Copy)]
pub(super) struct DialogSize {
    pub width: f32,
    pub body_height: f32,
    window_height: f32,
}

/// Clamp the preferred content width and leave room for fixed title/actions.
/// `chrome_height` includes the height of any wrapped action rows.
pub(super) fn dialog_size(ctx: &Context, preferred_width: f32, chrome_height: f32) -> DialogSize {
    let viewport = ctx.content_rect().size();
    let frame_clearance = 2. * (f32::from(CHROME_MARGIN) + 8.);
    DialogSize {
        width: preferred_width.min(viewport.x - frame_clearance).max(1.),
        body_height: (viewport.y - frame_clearance - chrome_height).max(1.),
        window_height: (viewport.y - frame_clearance).max(1.),
    }
}

/// Place only the form content in this scroll area, with the action row after it.
pub(super) fn dialog_body<T>(
    ui: &mut Ui,
    size: DialogSize,
    content: impl FnOnce(&mut Ui) -> T,
) -> egui::scroll_area::ScrollAreaOutput<T> {
    ScrollArea::vertical()
        .max_height(size.body_height)
        .auto_shrink([false, true])
        .show(ui, content)
}

/// Request focus once when a dialog or challenge is shown. The caller supplies
/// the challenge identity when consecutive challenges share the same widgets.
pub(super) fn focus_on_entry(ui: &Ui, response: &Response, owner: Id) -> bool {
    let available = response.enabled()
        && !ui.is_sizing_pass()
        && ui.memory(|m| m.allows_interaction(ui.layer_id()));
    let frame = ui.ctx().cumulative_frame_nr();
    let marker = owner.with((ui.ctx().viewport_id(), response.id, "entry-focus"));
    let entering = ui.data_mut(|data| {
        let previous = data.get_temp::<u64>(marker);
        let new = previous.is_none_or(|previous| previous.saturating_add(1) < frame);
        if available || !new {
            data.insert_temp(marker, frame);
        }
        available && new
    });
    if entering {
        response.request_focus();
    }
    entering
}

pub(super) fn select_name_on_entry(ui: &Ui, field: &Response, owner: Id, value: &str) {
    if focus_on_entry(ui, field, owner)
        && let Some(mut state) = TextEdit::load_state(ui.ctx(), field.id)
    {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(value.chars().count()),
            )));
        state.store(ui.ctx(), field.id);
    }
}

#[derive(Clone)]
struct ModalOwner {
    modal: Id,
    invoker: Option<Id>,
    pass: u64,
}

fn remember_modal_owner(ctx: &Context, modal: Id) {
    let installed = Id::new("fluent-modal-focus-callback");
    if !ctx.data(|data| data.get_temp::<bool>(installed).unwrap_or(false)) {
        ctx.data_mut(|data| data.insert_temp(installed, true));
        ctx.on_end_pass(
            "restore dismissed dialog focus",
            std::sync::Arc::new(|ui| {
                let ctx = ui.ctx();
                let key = Id::new(("fluent-modal-owners", ctx.viewport_id()));
                let pass = ctx.cumulative_pass_nr();
                let invoker = ctx.data_mut(|data| {
                    let owners = data.get_temp_mut_or_default::<Vec<ModalOwner>>(key);
                    let mut invoker = None;
                    while owners.last().is_some_and(|owner| owner.pass != pass) {
                        invoker = owners.pop().and_then(|owner| owner.invoker);
                    }
                    owners.retain(|owner| owner.pass == pass);
                    invoker
                });
                if let Some(response) = invoker.and_then(|id| ctx.read_response(id))
                    && response.enabled()
                {
                    response.request_focus();
                }
            }),
        );
    }
    let key = Id::new(("fluent-modal-owners", ctx.viewport_id()));
    let pass = ctx.cumulative_pass_nr();
    let focus = ctx.memory(|m| m.focused());
    ctx.data_mut(|data| {
        let owners = data.get_temp_mut_or_default::<Vec<ModalOwner>>(key);
        if let Some(owner) = owners.iter_mut().find(|owner| owner.modal == modal) {
            owner.pass = pass;
        } else {
            owners.push(ModalOwner {
                modal,
                invoker: focus,
                pass,
            });
        }
    });
}

/// A blocking egui modal. The caller decides whether Escape/backdrop activate a
/// safe dismissal and owns the focus and action semantics.
pub(super) fn modal<T>(
    ctx: &Context,
    id: Id,
    title: &str,
    size: DialogSize,
    content: impl FnOnce(&mut Ui) -> T,
) -> ModalResponse<T> {
    remember_modal_owner(ctx, id);
    let modal = Modal::new(id)
        .frame(Frame::window(&ctx.style_of(ctx.theme())))
        .show(ctx, |ui| {
            ui.set_width(size.width);
            ui.add(Label::new(RichText::new(title).size(DIALOG_TITLE).strong()).wrap());
            content(ui)
        });
    ctx.accesskit_node_builder(modal.response.id, |node| {
        node.set_role(egui::accesskit::Role::Dialog);
        node.set_modal();
        node.set_label(title);
    });
    modal
}

/// A bounded modeless window. Put long content in `dialog_body` inside it.
pub(super) fn modeless<'a>(ctx: &Context, id: Id, title: &'a str, size: DialogSize) -> Window<'a> {
    Window::new(RichText::new(title).size(DIALOG_TITLE))
        .id(id)
        .min_width(1.)
        .default_width(size.width)
        .max_width(size.width)
        .min_height(1.)
        .max_height(size.window_height)
        .constrain_to(ctx.content_rect())
}

/// A labelled field that fits the current column and keeps a 32-point hit area.
pub(super) fn field(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    preferred_width: f32,
) -> Response {
    let label = ui.add(Label::new(label).wrap());
    let width = ui.available_width().min(preferred_width).max(1.);
    let response = ui
        .add(
            TextEdit::singleline(value)
                .hint_text(hint)
                .desired_width(width)
                .min_size(vec2(0., CONTROL_HEIGHT))
                .margin(Margin::symmetric(8, 4)),
        )
        .labelled_by(label.id);
    if response.enabled() && response.has_focus() {
        let underline = response.rect.shrink2(vec2(4., 2.));
        if underline.is_positive() {
            ui.painter().line_segment(
                [underline.left_bottom(), underline.right_bottom()],
                Stroke::new(2., palette(ui).focus),
            );
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, ViewportId};

    fn contrast(a: Color32, b: Color32) -> f32 {
        fn luminance(c: Color32) -> f32 {
            let [r, g, b, _] = c.to_array();
            let linear = |channel: u8| {
                let s = f32::from(channel) / 255.;
                if s <= 0.04045 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
        }
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_remains_readable_on_surfaces_and_control_states() {
        for theme in [Theme::Light, Theme::Dark] {
            for high_contrast in [false, true] {
                let p = Palette::new(theme == Theme::Dark, high_contrast);
                for bg in [
                    p.surface, p.floating, p.control, p.hover, p.pressed, p.selected,
                ] {
                    for fg in [
                        p.text,
                        p.secondary_text,
                        p.accent,
                        p.error,
                        p.warning,
                        p.success,
                    ] {
                        assert!(
                            contrast(fg, bg) >= 4.5,
                            "{theme:?} {high_contrast}: {fg:?} on {bg:?} = {}",
                            contrast(fg, bg)
                        );
                    }
                }
                for bg in [p.primary, p.primary_hover, p.primary_pressed] {
                    assert!(contrast(p.on_primary, bg) >= 4.5);
                }
                assert!(contrast(p.focus, p.hover) >= 3.);
                if high_contrast {
                    assert!(contrast(p.border, p.surface) >= 7.);
                }
            }
        }
    }

    #[test]
    fn primary_paint_tracks_pointer_focus_and_availability_without_moving() {
        let ctx = Context::default();
        setup(&ctx, true, false);
        let mut rect = Rect::NOTHING;
        let mut run = |events: Vec<Event>, enabled: bool, focus: bool| {
            let mut clicked = false;
            let mut focused = false;
            let mut output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    if !enabled {
                        ui.disable();
                    }
                    let response = primary(ui, "Connect");
                    if focus {
                        response.request_focus();
                    }
                    rect = response.rect;
                    clicked = response.clicked();
                    focused = response.has_focus();
                },
            );
            output.textures_delta.clear();
            let fills: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(shape) if shape.rect.intersects(rect) => {
                        Some((shape.fill, shape.stroke.width))
                    }
                    _ => None,
                })
                .collect();
            assert!(!fills.is_empty());
            if enabled {
                let text_color = output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Connect" => {
                        text.override_text_color
                    }
                    _ => None,
                });
                assert!(contrast(text_color.expect("painted label color"), fills[0].0) >= 4.5);
            }
            (rect, fills, clicked, focused)
        };
        let (initial, rest, _, _) = run(vec![], true, false);
        let point = initial.center();
        let pointer = |pressed| Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let (hover_rect, hovered, _, _) = run(vec![Event::PointerMoved(point)], true, false);
        let (press_rect, pressed, clicked, _) = run(vec![pointer(true)], true, false);
        assert!(!clicked);
        let (release_rect, _, clicked, _) = run(vec![pointer(false)], true, false);
        assert!(clicked);
        assert_eq!(initial, hover_rect);
        assert_eq!(initial, press_rect);
        assert_eq!(initial, release_rect);
        assert_ne!(rest[0].0, hovered[0].0);
        assert_ne!(hovered[0].0, pressed[0].0);
        run(vec![Event::PointerGone], true, true);
        let (_, focused, _, has_focus) = run(vec![], true, false);
        assert!(has_focus);
        assert!(focused.iter().any(|(_, width)| *width >= 2.));
        let (_, _, activated, _) = run(
            vec![Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            true,
            false,
        );
        assert!(activated);
        let (disabled_rect, disabled, clicked, _) = run(
            vec![Event::PointerMoved(point), pointer(true), pointer(false)],
            false,
            false,
        );
        assert_eq!(initial, disabled_rect);
        assert_ne!(rest[0].0, disabled[0].0);
        assert!(!clicked);
    }

    #[test]
    fn primary_label_keeps_contrast_at_pointer_highlight_peak() {
        for theme in [Theme::Light, Theme::Dark] {
            let ctx = Context::default();
            ctx.set_theme(theme);
            setup(&ctx, false, false);
            let render = |events| {
                let mut output = ctx.run_ui(
                    RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        primary(ui, "Connect");
                    },
                );
                output.textures_delta.clear();
                output.shapes
            };
            let rest = render(vec![]);
            let point = rest
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Connect" => {
                        let row = &text.galley.rows[0];
                        let glyph = &row.row.glyphs[3];
                        Some(text.pos + row.pos.to_vec2() + glyph.logical_rect().center().to_vec2())
                    }
                    _ => None,
                })
                .expect("point under the primary label");

            let mut background = None;
            let mut illuminated = false;
            let mut label = None;
            for shape in render(vec![Event::PointerMoved(point)]) {
                if !shape.clip_rect.contains(point) {
                    continue;
                }
                match shape.shape {
                    egui::Shape::Rect(rect) if rect.rect.contains(point) => {
                        background = Some(rect.fill);
                    }
                    egui::Shape::Mesh(mesh) => {
                        let peak = mesh.vertices.iter().find(|vertex| vertex.pos == point);
                        if let Some(vertex) = peak {
                            assert!(vertex.color.a() > 0);
                            background = background.map(|fill| fill.blend(vertex.color));
                            illuminated = true;
                        }
                    }
                    egui::Shape::Text(text) if text.galley.text() == "Connect" => {
                        label = text.override_text_color;
                        break;
                    }
                    _ => {}
                }
            }
            assert!(
                illuminated,
                "{theme:?}: expected a pointer highlight beneath the label"
            );
            let ratio = contrast(label.unwrap(), background.unwrap());
            assert!(
                ratio >= 4.5,
                "{theme:?}: contrast at the pointer is {ratio:.2}:1"
            );
        }
    }

    #[test]
    fn selected_control_retains_selection_when_pointer_leaves() {
        let ctx = Context::default();
        setup(&ctx, false, false);
        let mut rect = Rect::NOTHING;
        let mut run = |selected, events: Vec<Event>| {
            let mut output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    let (bounds, response) =
                        ui.allocate_exact_size(vec2(120., CONTROL_HEIGHT), Sense::click());
                    rect = bounds;
                    paint_control(ui, &response, rect, selected, ControlKind::Standard);
                },
            );
            output.textures_delta.clear();
            let mut fill = Color32::TRANSPARENT;
            let mut reveal = false;
            for shape in output.shapes {
                match shape.shape {
                    egui::Shape::Rect(shape) if shape.rect == rect => fill = shape.fill,
                    egui::Shape::Mesh(mesh) => {
                        reveal = true;
                        assert!(rect.contains_rect(shape.clip_rect));
                        assert!(mesh.calc_bounds().size().max_elem() <= 36.1);
                    }
                    _ => {}
                }
            }
            (rect, fill, reveal)
        };
        let (bounds, rest, _) = run(false, vec![]);
        let (_, selected, _) = run(true, vec![]);
        assert_ne!(rest, selected);
        let (_, hovered, reveal) = run(true, vec![Event::PointerMoved(bounds.center())]);
        assert_ne!(selected, hovered);
        assert!(reveal);
        let (_, left, reveal) = run(
            true,
            vec![Event::PointerMoved(bounds.right_bottom() + vec2(8., 8.))],
        );
        assert_eq!(selected, left);
        assert!(!reveal);
    }

    #[test]
    fn scaled_narrow_dialog_keeps_field_and_footer_inside_viewport() {
        for scale in [1., 1.5, 2.] {
            let ctx = Context::default();
            setup(&ctx, true, false);
            let mut input = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    vec2(560. / scale, 440. / scale),
                )),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(scale);
            let mut field_rect = Rect::NOTHING;
            let mut footer_rect = Rect::NOTHING;
            let mut body_rect = Rect::NOTHING;
            let mut dialog_rect = Rect::NOTHING;
            let mut value =
                "a very long saved host name that should scroll inside the input".to_owned();
            let draw = |ui: &mut Ui| {
                let ctx = ui.ctx();
                let size = dialog_size(ctx, 480., 104.);
                dialog_rect = modal(
                    ctx,
                    Id::new("test-dialog"),
                    "Session settings",
                    size,
                    |ui| {
                        body_rect = dialog_body(ui, size, |ui| {
                            field_rect = field(ui, "Hostname", &mut value, "Address", 400.).rect;
                            for i in 0..24 {
                                ui.label(format!("Optional setting {i}"));
                            }
                        })
                        .inner_rect;
                        footer_rect = primary(ui, "Cancel").rect;
                    },
                )
                .response
                .rect;
            };
            let mut draw = draw;
            ctx.run_ui(input.clone(), &mut draw).textures_delta.clear();
            ctx.run_ui(input, &mut draw).textures_delta.clear();
            assert!(
                ctx.content_rect().expand(1.).contains_rect(dialog_rect),
                "{scale}: {dialog_rect:?} in {:?}",
                ctx.content_rect()
            );
            assert!(body_rect.expand(1.).contains_rect(field_rect));
            assert!(field_rect.height() >= 32.);
            assert!(footer_rect.top() >= body_rect.bottom());
            assert!(footer_rect.height() >= 32.);
            assert!(ctx.content_rect().contains_rect(footer_rect));
        }
    }

    #[test]
    fn modal_keyboard_traversal_and_backdrop_keep_the_background_control_inactive() {
        let ctx = Context::default();
        let mut controls = [Id::NULL; 2];
        let mut background = Rect::NOTHING;
        let mut clicks = 0;
        let mut draw = |events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(720., 480.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let behind = ui.button("Background");
                    background = behind.rect;
                    clicks += usize::from(behind.clicked());
                    modal(
                        ui.ctx(),
                        Id::new("tab-trap"),
                        "Confirm",
                        dialog_size(ui.ctx(), 260., 100.),
                        |ui| {
                            let approve = ui.button("Approve");
                            let reject = ui.button("Reject");
                            controls = [approve.id, reject.id];
                            focus_on_entry(ui, &reject, ui.id());
                        },
                    );
                },
            );
            output.textures_delta.clear();
            (controls, background, clicks)
        };
        draw(vec![]);
        let (buttons, behind, _) = draw(vec![]);
        assert_eq!(ctx.memory(|m| m.focused()), Some(buttons[1]));
        for shift in [false, false, true, true] {
            draw(vec![Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers {
                    shift,
                    ..Default::default()
                },
            }]);
            draw(vec![Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers {
                    shift,
                    ..Default::default()
                },
            }]);
            assert!(
                ctx.memory(|m| m.focused())
                    .is_some_and(|id| buttons.contains(&id))
            );
        }
        for pressed in [true, false] {
            let (_, _, clicks) = draw(vec![
                Event::PointerMoved(behind.center()),
                Event::PointerButton {
                    pos: behind.center(),
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
            assert_eq!(clicks, 0);
        }
    }

    #[test]
    fn modal_accessibility_tree_names_the_dialog_and_masks_secret_fields() {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut secret = String::from("synthetic-test-value");
        let mut update = None;
        for _ in 0..2 {
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                modal(
                    ui.ctx(),
                    Id::new("auth-test"),
                    "Sign in",
                    dialog_size(ui.ctx(), 380., 100.),
                    |ui| {
                        let label = ui.label("Password");
                        ui.add(TextEdit::singleline(&mut secret).password(true))
                            .labelled_by(label.id);
                        ui.button("Cancel").widget_info(|| {
                            WidgetInfo::labeled(WidgetType::Button, true, "Cancel")
                        });
                    },
                );
            });
            output.textures_delta.clear();
            update = output.platform_output.accesskit_update;
        }
        let nodes = update.unwrap().nodes;
        let (_, dialog) = nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::Dialog)
            .unwrap();
        assert!(dialog.is_modal());
        assert_eq!(dialog.label(), Some("Sign in"));
        let (_, password) = nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::PasswordInput)
            .unwrap();
        assert!(!password.labelled_by().is_empty());
        assert!(
            nodes
                .iter()
                .all(|(_, node)| !node.value().unwrap_or_default().contains(&secret))
        );
    }

    #[test]
    fn reduced_motion_keeps_native_scrolling_but_removes_transitions() {
        let ctx = Context::default();
        setup(&ctx, false, false);
        for theme in [Theme::Light, Theme::Dark] {
            assert!(ctx.style_of(theme).animation_time > 0.);
        }
        setup(&ctx, true, true);
        for theme in [Theme::Light, Theme::Dark] {
            let style = ctx.style_of(theme);
            assert_eq!(style.animation_time, 0.);
            assert_eq!(style.scroll_animation, egui::style::ScrollAnimation::none());
            assert!(style.spacing.scroll.floating);
            assert!(style.spacing.scroll.floating_width < style.spacing.scroll.bar_width);
            assert!(
                style.spacing.scroll.floating_allocated_width >= style.spacing.scroll.bar_width
            );
        }
    }
}
