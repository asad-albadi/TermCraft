use super::*;
use harbor_workspace::terminal::highlight::{HighlightRules, MAX_RULES};
#[derive(Default)]
pub(super) struct Cache {
    source: Option<Vec<HighlightRule>>,
    pub compiled: HighlightRules,
    pub error: Option<String>,
}
impl Cache {
    pub fn update(&mut self, rules: &[HighlightRule]) {
        if let Err(error) = HighlightRules::validate_limits(rules) {
            self.source = None;
            self.error = Some(error);
            return;
        }
        if self.source.as_deref() == Some(rules) {
            return;
        }
        self.source = Some(rules.to_vec());
        match HighlightRules::compile(rules) {
            Ok(compiled) => {
                self.compiled = compiled;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}
pub(super) struct Editor {
    pub rules: Vec<HighlightRule>,
    pub enabled: bool,
    error: String,
}
impl Editor {
    pub fn from_preferences(p: &Preferences) -> Self {
        if let Err(error) = HighlightRules::validate_limits(&p.highlight_rules) {
            return Self {
                rules: Vec::new(),
                enabled: p.highlight_enabled,
                error: format!(
                    "Saved rules exceed the editor limits: {error} This draft starts empty; Cancel preserves saved settings."
                ),
            };
        }
        Self {
            rules: p.highlight_rules.as_ref().clone(),
            enabled: p.highlight_enabled,
            error: String::new(),
        }
    }
    pub fn apply(&mut self, p: &mut Preferences) -> Result<(), String> {
        p.set_highlight_rules(self.rules.clone())?;
        p.highlight_enabled = self.enabled;
        Ok(())
    }
}
fn color(ui: &mut Ui, label: &str, value: &mut Option<[u8; 3]>) {
    ui.horizontal(|ui| {
        let mut enabled = value.is_some();
        if ui.checkbox(&mut enabled, label).changed() {
            *value = enabled.then_some([128, 128, 128]);
        }
        if let Some(rgb) = value {
            ui.color_edit_button_srgb(rgb);
        }
    });
}
fn style(ui: &mut Ui, label: &str, style: &mut HighlightStyle) {
    ui.label(label);
    color(ui, "Text color", &mut style.foreground);
    color(ui, "Background", &mut style.background);
}
impl Workspace {
    pub(super) fn highlighting_dialog(&mut self, ctx: &Context) {
        let Some(mut editor) = self.highlight_editor.take() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let size = fluent::dialog_size(ctx, 610., 148.);
        fluent::modeless(ctx, Id::new("highlight-rules"), "Terminal highlighting", size).open(&mut open).show(ctx,|ui|{
            let mut remove=None;
            fluent::dialog_body(ui,size,|ui|{
            ui.checkbox(&mut editor.enabled,"Enable globally (sessions can override)");
            ui.label("Higher priority wins; the first rule wins ties. Search and selection draw above keyword colors. Unmatched text keeps application colors.");
            ui.small("Up to 16 rules, 256 bytes per pattern. Each visible row matches its first 8 KiB, up to 128 matches per rule. Regular expressions use Rust regex syntax without look-around or backreferences.");

                for (index,rule) in editor.rules.iter_mut().enumerate() {
                    ui.push_id(index,|ui|{ui.group(|ui|{
                        ui.horizontal(|ui|{ui.checkbox(&mut rule.enabled,"Enabled");ui.label("Priority");ui.add(DragValue::new(&mut rule.precedence));if ui.small_button("Remove").clicked(){remove=Some(index);}});
                        ui.horizontal(|ui|{ui.selectable_value(&mut rule.kind,HighlightMatch::Literal,"Literal");ui.selectable_value(&mut rule.kind,HighlightMatch::Regex,"Regular expression");ui.checkbox(&mut rule.case_sensitive,"Match case");});
                        ui.add(TextEdit::singleline(&mut rule.pattern).desired_width(f32::INFINITY).char_limit(256));
                        if ui.available_width() < 440. {style(ui,"Light appearance",&mut rule.light);style(ui,"Dark appearance",&mut rule.dark);} else {ui.columns(2,|cols|{style(&mut cols[0],"Light appearance",&mut rule.light);style(&mut cols[1],"Dark appearance",&mut rule.dark);});}
                    });});
                }
            });
            if let Some(index)=remove {editor.rules.remove(index);}
            if ui.add_enabled(editor.rules.len()<MAX_RULES,Button::new("Add rule")).clicked(){editor.rules.push(HighlightRule::default());}
            if !editor.error.is_empty(){crate::ui::errors::show(ui, crate::ui::errors::Domain::Highlight, &editor.error);}
            ui.horizontal(|ui|{save=primary(ui,"Save highlighting").clicked();cancel=ui.button("Cancel").clicked();});
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
            self.highlight_editor = Some(editor);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_saved_rules_are_not_cloned_into_render_caches() {
        let mut cache = Cache::default();
        cache.update(&[HighlightRule::default()]);
        let oversized = vec![HighlightRule {
            pattern: "x".repeat(1024 * 1024),
            ..Default::default()
        }];
        cache.update(&oversized);
        assert!(cache.error.is_some());
        assert_eq!(cache.compiled.spans("ERROR").len(), 1);
        assert!(
            cache.source.as_ref().is_none_or(
                |rules| rules.len() <= 16 && rules.iter().all(|r| r.pattern.len() <= 256)
            ),
            "rejected input must not be retained per tab"
        );
        let p: Preferences =
            serde_json::from_value(serde_json::json!({"highlight_rules":oversized})).unwrap();
        let cloned = p.clone();
        assert_eq!(
            p.highlight_rules.as_ptr(),
            cloned.highlight_rules.as_ptr(),
            "render preference clones must share saved rule storage"
        );
        let editor = Editor::from_preferences(&p);
        assert!(editor.rules.is_empty());
        assert!(!editor.error.is_empty());
    }
    #[test]
    fn malformed_import_retains_cached_working_rules_until_valid_replacement() {
        let mut cache = Cache::default();
        cache.update(&[HighlightRule::default()]);
        assert_eq!(cache.compiled.spans("ERROR").len(), 1);
        cache.update(&[HighlightRule {
            pattern: "[".into(),
            kind: HighlightMatch::Regex,
            ..Default::default()
        }]);
        assert!(cache.error.is_some());
        assert_eq!(cache.compiled.spans("ERROR").len(), 1);
        cache.update(&[]);
        assert!(cache.error.is_none());
        assert!(cache.compiled.spans("ERROR").is_empty());
    }
    #[test]
    fn invalid_editor_draft_does_not_replace_saved_rules_or_enabled_state() {
        let mut p = Preferences::default();
        p.set_highlight_rules(vec![HighlightRule::default()])
            .unwrap();
        let before = p.highlight_rules.clone();
        let mut editor = Editor::from_preferences(&p);
        editor.enabled = false;
        editor.rules[0].kind = HighlightMatch::Regex;
        editor.rules[0].pattern = "[".into();
        assert!(editor.apply(&mut p).is_err());
        assert_eq!(p.highlight_rules, before);
        assert!(p.highlight_enabled);
    }
}
