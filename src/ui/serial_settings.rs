use super::*;
pub(super) fn session(ui: &mut Ui, s: &mut Session) {
    ui.label("Data bits");
    ui.add(DragValue::new(&mut s.serial_line.data_bits).range(5..=8));
    ui.end_row();
    ui.label("Parity");
    ui.horizontal(|ui| {
        use harbor_workspace::serial::Parity;
        for (p, label) in [
            (Parity::None, "None"),
            (Parity::Even, "Even"),
            (Parity::Odd, "Odd"),
        ] {
            ui.selectable_value(&mut s.serial_line.parity, p, label);
        }
    });
    ui.end_row();
    ui.label("Stop bits");
    ui.add(DragValue::new(&mut s.serial_line.stop_bits).range(1..=2));
    ui.end_row();
    ui.label("Flow control");
    ui.horizontal_wrapped(|ui| {
        use harbor_workspace::serial::FlowControl;
        for (flow, label) in [
            (FlowControl::None, "None"),
            (FlowControl::Software, "XON/XOFF"),
            (FlowControl::Hardware, "RTS/CTS"),
        ] {
            ui.selectable_value(&mut s.serial_line.flow_control, flow, label);
        }
    });
    ui.end_row();
}
#[cfg(test)]
mod tests {
    use super::*;
    fn label(shape: &egui::epaint::Shape, value: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == value => {
                Some(t.pos + t.galley.size() / 2.)
            }
            egui::epaint::Shape::Vec(items) => items.iter().find_map(|s| label(s, value)),
            _ => None,
        }
    }
    #[test]
    fn serial_controls_change_parity_and_flow_control() {
        let ctx = Context::default();
        let mut s = Session {
            protocol: "Serial".into(),
            ..Default::default()
        };
        for target in ["Even", "RTS/CTS"] {
            let render = |ui: &mut Ui, s: &mut Session| {
                Grid::new("serial-test")
                    .num_columns(2)
                    .show(ui, |ui| session(ui, s));
            };
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 800.))),
                    ..Default::default()
                },
                |ui| render(ui, &mut s),
            );
            let point = output
                .shapes
                .iter()
                .find_map(|item| label(&item.shape, target))
                .unwrap();
            output.textures_delta.clear();
            for pressed in [true, false] {
                let mut clicked = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 800.))),
                        events: vec![
                            Event::PointerMoved(point),
                            Event::PointerButton {
                                pos: point,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::default(),
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| render(ui, &mut s),
                );
                clicked.textures_delta.clear();
            }
        }
        assert_eq!(s.serial_line.parity, harbor_workspace::serial::Parity::Even);
        assert_eq!(
            s.serial_line.flow_control,
            harbor_workspace::serial::FlowControl::Hardware
        );
    }
}
