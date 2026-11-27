use super::*;
use harbor_workspace::terminal::settings::{Dimensions, TerminalFont, TerminalSettings};
impl TerminalTab {
    pub(super) fn apply_terminal_preferences(&mut self, p: &Preferences) {
        let mut settings =
            TerminalSettings::resolve(&p.terminal_settings(), &self.session.terminal);
        if self.paste.is_some() || self.paste_end.is_some() || self.after_paste.is_some() {
            settings.encoding = self.codec.encoding();
        }
        let flush = self.codec.set_encoding(settings.encoding);
        if !flush.is_empty() {
            self.screen.feed(flush.as_bytes());
        }
        if let Dimensions::Fixed { cols, rows } = settings.dimensions {
            if self.screen.cols != cols as usize || self.screen.rows != rows as usize {
                self.screen.resize(cols as usize, rows as usize);
                self.resize_transport();
            }
        }
        self.terminal_settings = settings;
    }
    pub(super) fn resize_transport(&mut self) {
        if let Some(legacy) = &self.legacy {
            if let Err(error) = legacy.resize(self.screen.cols as u16, self.screen.rows as u16) {
                self.error = error;
            }
        }
        if let Some(process) = &self.process {
            process.resize(self.screen.cols as u16, self.screen.rows as u16);
        }
        if let Some(ssh) = &self.ssh {
            if let Err(error) = ssh.try_send(harbor_workspace::ssh::ConnectionCommand::Resize {
                cols: self.screen.cols as u32,
                rows: self.screen.rows as u32,
            }) {
                self.error = error;
            }
        }
    }
    pub(super) fn send_user_bytes(&mut self, bytes: &[u8], record: bool) -> Result<(), String> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "Text input must be UTF-8 before terminal encoding.".to_string())?;
        let encoded = self.codec.encode(text)?;
        self.transmit(&encoded, false)?;
        if record {
            self.record_macro_text(text);
            if self.recording {
                self.recorded.push_str(text);
            }
        }
        Ok(())
    }
}
pub(super) fn font_family(font: TerminalFont) -> FontFamily {
    match font {
        TerminalFont::DejaVuSansMono => FontFamily::Name("DejaVu Sans Mono".into()),
        TerminalFont::LiberationSans => FontFamily::Name("Liberation Sans".into()),
        TerminalFont::MonospaceFallback => FontFamily::Monospace,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_editor_migrates_legacy_font_and_rejects_invalid_draft() {
        let mut p: Preferences = serde_json::from_str(r#"{"font_size":19.0}"#).unwrap();
        let mut editor = ProfileEditor::from_preferences(&p);
        assert_eq!(editor.settings.font_size, 19.);
        editor.settings.font_size = f32::NAN;
        assert!(editor.apply(&mut p).is_err());
        assert!(p.terminal.is_none());
        assert_eq!(p.font_size, 19.);
        editor.settings.font_size = 17.;
        editor.apply(&mut p).unwrap();
        assert_eq!(p.terminal_settings().font_size, 17.);
        assert_eq!(p.font_size, 17.);
    }
}
use harbor_workspace::terminal::settings::{
    BackspaceMapping, CharacterEncoding, TerminalOverrides, TerminalPalette,
};
pub(super) struct ProfileEditor {
    pub settings: TerminalSettings,
    error: String,
}
impl ProfileEditor {
    pub fn from_preferences(p: &Preferences) -> Self {
        Self {
            settings: p.terminal_settings(),
            error: String::new(),
        }
    }
    pub fn apply(&mut self, p: &mut Preferences) -> Result<(), String> {
        if !self.settings.font_size.is_finite() || !(8.0..=32.0).contains(&self.settings.font_size)
        {
            return Err("Choose a font size between 8 and 32 px.".into());
        }
        self.settings = TerminalSettings::resolve(&self.settings, &TerminalOverrides::default());
        p.font_size = self.settings.font_size;
        p.terminal = Some(self.settings.clone());
        Ok(())
    }
}
fn fonts(ui: &mut Ui, value: &mut TerminalFont) {
    ui.horizontal_wrapped(|ui| {
        for (font, label) in [
            (TerminalFont::DejaVuSansMono, "DejaVu Sans Mono"),
            (TerminalFont::LiberationSans, "Liberation Sans"),
            (TerminalFont::MonospaceFallback, "Monospace fallback"),
        ] {
            ui.selectable_value(value, font, label);
        }
    });
}
fn encoding(ui: &mut Ui, value: &mut CharacterEncoding) {
    ui.horizontal_wrapped(|ui| {
        for item in [
            CharacterEncoding::Utf8,
            CharacterEncoding::Windows1252,
            CharacterEncoding::ShiftJis,
        ] {
            ui.selectable_value(value, item, item.label());
        }
    });
}
fn backspace(ui: &mut Ui, value: &mut BackspaceMapping) {
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(value, BackspaceMapping::Delete, "DEL (0x7f)");
        ui.selectable_value(value, BackspaceMapping::ControlH, "Ctrl+H (0x08)");
    });
}
fn dimensions(ui: &mut Ui, value: &mut Dimensions) {
    let mut fixed = matches!(value, Dimensions::Fixed { .. });
    if ui.checkbox(&mut fixed, "Fixed columns and rows").changed() {
        *value = if fixed {
            Dimensions::Fixed { cols: 80, rows: 24 }
        } else {
            Dimensions::Dynamic
        };
    }
    if let Dimensions::Fixed { cols, rows } = value {
        ui.horizontal_wrapped(|ui| {
            ui.label("Columns");
            ui.add(DragValue::new(cols).range(2..=500));
            ui.label("Rows");
            ui.add(DragValue::new(rows).range(2..=200));
        });
    }
}
fn palette(ui: &mut Ui, value: &mut TerminalPalette) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Default text");
        ui.color_edit_button_srgb(&mut value.foreground);
        ui.label("Background");
        ui.color_edit_button_srgb(&mut value.background);
    });
    let columns = if ui.available_width() < 400. { 4 } else { 8 };
    Grid::new("ansi-palette")
        .num_columns(columns)
        .show(ui, |ui| {
            for (index, rgb) in value.ansi.iter_mut().enumerate() {
                ui.vertical(|ui| {
                    ui.small(index.to_string());
                    ui.color_edit_button_srgb(rgb);
                });
                if index % columns == columns - 1 {
                    ui.end_row();
                }
            }
        });
}
fn override_value<T: Clone>(
    ui: &mut Ui,
    label: &str,
    value: &mut Option<T>,
    global: &T,
    edit: impl FnOnce(&mut Ui, &mut T),
) {
    let mut enabled = value.is_some();
    if ui.checkbox(&mut enabled, label).changed() {
        *value = enabled.then(|| global.clone());
    }
    if let Some(value) = value {
        ui.indent(label, |ui| edit(ui, value));
    }
}
pub(super) fn overrides(ui: &mut Ui, value: &mut TerminalOverrides, global: &TerminalSettings) {
    if ui
        .button("Reset all terminal overrides to global")
        .clicked()
    {
        value.reset();
    }
    override_value(
        ui,
        "Override font family",
        &mut value.font_family,
        &global.font_family,
        fonts,
    );
    override_value(
        ui,
        "Override font size",
        &mut value.font_size,
        &global.font_size,
        |ui, v| {
            ui.add(Slider::new(v, 8.0..=32.).suffix(" px"));
        },
    );
    override_value(
        ui,
        "Override encoding",
        &mut value.encoding,
        &global.encoding,
        encoding,
    );
    override_value(
        ui,
        "Override Backspace",
        &mut value.backspace,
        &global.backspace,
        backspace,
    );
    override_value(
        ui,
        "Override dimensions",
        &mut value.dimensions,
        &global.dimensions,
        dimensions,
    );
    CollapsingHeader::new("Palette overrides").show(ui, |ui| {
        ui.push_id("light", |ui| {
            override_value(
                ui,
                "Override light palette",
                &mut value.light_palette,
                &global.light_palette,
                palette,
            )
        });
        ui.push_id("dark", |ui| {
            override_value(
                ui,
                "Override dark palette",
                &mut value.dark_palette,
                &global.dark_palette,
                palette,
            )
        });
    });
}
impl Workspace {
    pub(super) fn terminal_profile_dialog(&mut self, ctx: &Context) {
        let Some(mut editor) = self.terminal_profile_editor.take() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let size = fluent::dialog_size(ctx, 680., 148.);
        fluent::modeless(ctx, Id::new("global-terminal-profile"), "Global terminal profile", size).open(&mut open).show(ctx,|ui|{
            fluent::dialog_body(ui,size,|ui|{
                ui.small("Saving updates new and open terminals wherever the option inherits the global profile.");
                ui.label("Font family (bundled fonts and configured fallback)");fonts(ui,&mut editor.settings.font_family);ui.add(Slider::new(&mut editor.settings.font_size,8.0..=32.).suffix(" px"));
                ui.label("Character encoding");encoding(ui,&mut editor.settings.encoding);ui.small("Other encodings, UTF-16/stateful control encodings and arbitrary OS font discovery are not supported. Encoding changes affect new bytes; retained output and raw logs are preserved. An encoding change waits for a running paste to finish.");
                ui.label("Backspace sends");backspace(ui,&mut editor.settings.backspace);dimensions(ui,&mut editor.settings.dimensions);
                ui.push_id("light",|ui|{CollapsingHeader::new("Light palette").show(ui,|ui|palette(ui,&mut editor.settings.light_palette));});ui.push_id("dark",|ui|{CollapsingHeader::new("Dark palette").show(ui,|ui|palette(ui,&mut editor.settings.dark_palette));});
            });
            if !editor.error.is_empty(){ui.colored_label(ui.visuals().error_fg_color,&editor.error);}
            ui.horizontal_wrapped(|ui|{save=primary(ui,"Save global profile").clicked();cancel=ui.button("Cancel").clicked();});
        });
        if save {
            match editor.apply(&mut self.store.preferences) {
                Ok(()) => {
                    self.persist();
                    return;
                }
                Err(error) => editor.error = error,
            }
        }
        if open && !cancel {
            self.terminal_profile_editor = Some(editor);
        }
    }
}
