//! Bounded display-only rules. Matching never changes terminal cells or exported bytes.
use crate::model::{HighlightMatch, HighlightRule, HighlightStyle};
use regex::{Regex, RegexBuilder};
use std::ops::Range;
pub const MAX_RULES: usize = 16;
pub const MAX_PATTERN_BYTES: usize = 256;
pub const MAX_TEXT_BYTES: usize = 8192;
pub const MAX_MATCHES: usize = 128;
struct Compiled {
    regex: Regex,
    rule: HighlightRule,
}
#[derive(Default)]
pub struct HighlightRules {
    rules: Vec<Compiled>,
}
#[derive(Clone, Debug)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub light: HighlightStyle,
    pub dark: HighlightStyle,
}
impl HighlightRules {
    pub fn validate_limits(rules: &[HighlightRule]) -> Result<(), String> {
        if rules.len() > MAX_RULES {
            return Err(format!("Use at most {MAX_RULES} highlighting rules."));
        }
        for (index, rule) in rules.iter().enumerate() {
            if rule.pattern.is_empty() || rule.pattern.len() > MAX_PATTERN_BYTES {
                return Err(format!(
                    "Rule {}: pattern must contain 1–{MAX_PATTERN_BYTES} UTF-8 bytes.",
                    index + 1
                ));
            }
        }
        Ok(())
    }
    pub fn compile(rules: &[HighlightRule]) -> Result<Self, String> {
        Self::validate_limits(rules)?;
        if rules.len() > MAX_RULES {
            return Err(format!("Use at most {MAX_RULES} highlighting rules."));
        }
        let mut compiled = Vec::new();
        for (index, rule) in rules.iter().enumerate() {
            if rule.pattern.is_empty() || rule.pattern.len() > MAX_PATTERN_BYTES {
                return Err(format!(
                    "Rule {}: pattern must contain 1–{MAX_PATTERN_BYTES} UTF-8 bytes.",
                    index + 1
                ));
            }
            let pattern = match rule.kind {
                HighlightMatch::Literal => regex::escape(&rule.pattern),
                HighlightMatch::Regex => rule.pattern.clone(),
            };
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(!rule.case_sensitive)
                .size_limit(128 * 1024)
                .dfa_size_limit(256 * 1024)
                .nest_limit(32)
                .build()
                .map_err(|e| format!("Rule {}: {e}", index + 1))?;
            if rule.enabled {
                compiled.push(Compiled {
                    regex,
                    rule: rule.clone(),
                });
            }
        }
        compiled.sort_by(|a, b| b.rule.precedence.cmp(&a.rule.precedence));
        Ok(Self { rules: compiled })
    }
    pub fn spans(&self, text: &str) -> Vec<HighlightSpan> {
        if self.rules.is_empty() {
            return Vec::new();
        }
        let mut end = text.len().min(MAX_TEXT_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let text = &text[..end];
        let mut owners = vec![u8::MAX; end];
        for (index, compiled) in self.rules.iter().enumerate() {
            for m in compiled.regex.find_iter(text).take(MAX_MATCHES) {
                for owner in &mut owners[m.range()] {
                    if *owner == u8::MAX {
                        *owner = index as u8;
                    }
                }
            }
        }
        let mut spans = Vec::new();
        let mut start = 0;
        while start < owners.len() {
            let owner = owners[start];
            let mut end = start + 1;
            while end < owners.len() && owners[end] == owner {
                end += 1;
            }
            if owner != u8::MAX {
                let rule = &self.rules[owner as usize].rule;
                spans.push(HighlightSpan {
                    range: start..end,
                    light: rule.light,
                    dark: rule.dark,
                });
            }
            start = end;
        }
        spans
    }
}
