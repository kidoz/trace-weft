use crate::{RedactionResult, RedactionStatus, Redactor};
use regex::Regex;
use std::sync::Arc;

pub struct RegexRedactor {
    patterns: Vec<Regex>,
}

impl RegexRedactor {
    pub fn new() -> Self {
        Self { patterns: vec![] }
    }

    pub fn with_default_patterns() -> Self {
        let mut redactor = Self::new();
        // Basic examples: email, simple API keys
        if let Ok(email_re) = Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}") {
            redactor.patterns.push(email_re);
        }
        if let Ok(key_re) = Regex::new(r"(?i)(sk-[a-zA-Z0-9]{32,})") {
            redactor.patterns.push(key_re);
        }
        if let Ok(bearer_re) = Regex::new(r"(?i)(bearer\s+[a-zA-Z0-9\-\._~+/\\]+)") {
            redactor.patterns.push(bearer_re);
        }
        redactor
    }

    pub fn add_pattern(&mut self, pattern: Regex) {
        self.patterns.push(pattern);
    }
}

impl Default for RegexRedactor {
    fn default() -> Self {
        Self::with_default_patterns()
    }
}

impl Redactor for RegexRedactor {
    fn redact(&self, input: &str) -> RedactionResult {
        if self.patterns.is_empty() {
            return RedactionResult {
                redacted_text: input.to_string(),
                status: RedactionStatus::Unredacted,
            };
        }

        let mut current_text = input.to_string();
        let mut was_redacted = false;

        for re in &self.patterns {
            let replaced = re.replace_all(&current_text, "[REDACTED]");
            if replaced != current_text {
                was_redacted = true;
                current_text = replaced.to_string();
            }
        }

        RedactionResult {
            redacted_text: current_text,
            status: if was_redacted {
                RedactionStatus::Redacted
            } else {
                RedactionStatus::Unredacted
            },
        }
    }
}

pub type ArcRedactor = Arc<dyn Redactor>;
