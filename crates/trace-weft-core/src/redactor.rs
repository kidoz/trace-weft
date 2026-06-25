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
        if let Ok(email_re) = Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}") {
            redactor.patterns.push(email_re);
        }
        if let Ok(key_re) = Regex::new(r"(?i)(sk-[a-zA-Z0-9]{32,})") {
            redactor.patterns.push(key_re);
        }
        if let Ok(bearer_re) = Regex::new(r"(?i)(bearer\s+[a-zA-Z0-9\-\._~+/\\]+)") {
            redactor.patterns.push(bearer_re);
        }
        if let Ok(secret_assignment_re) = Regex::new(
            r#"(?i)\b(?:api[_-]?key|secret|token|client[_-]?secret)\s*[:=]\s*["']?[a-z0-9_\-]{16,}["']?"#,
        ) {
            redactor.patterns.push(secret_assignment_re);
        }
        if let Ok(phone_re) =
            Regex::new(r"\+?(?:\d{1,3}[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?)\d{3}[-.\s]?\d{4}\b")
        {
            redactor.patterns.push(phone_re);
        }
        if let Ok(card_re) = Regex::new(r"\b(?:\d[ -]*?){13,19}\b") {
            redactor.patterns.push(card_re);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> RegexRedactor {
        RegexRedactor::with_default_patterns()
    }

    #[test]
    fn redacts_email_addresses() {
        let result = redactor().redact("contact me at jane.doe+test@example.co.uk please");
        assert_eq!(result.redacted_text, "contact me at [REDACTED] please");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn redacts_api_keys() {
        let key = format!("sk-{}", "a1B2".repeat(10));
        let result = redactor().redact(&format!("key={key}"));
        assert_eq!(result.redacted_text, "key=[REDACTED]");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn keeps_short_sk_prefixed_words() {
        let result = redactor().redact("see sk-short for details");
        assert_eq!(result.redacted_text, "see sk-short for details");
        assert_eq!(result.status, RedactionStatus::Unredacted);
    }

    #[test]
    fn redacts_bearer_tokens() {
        let result = redactor().redact("Authorization: Bearer abc.DEF-123~xyz");
        assert_eq!(result.redacted_text, "Authorization: [REDACTED]");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn redacts_secret_assignments() {
        let result = redactor().redact("api_key = tw_abcdefghijklmnopqrstuvwxyz");
        assert_eq!(result.redacted_text, "[REDACTED]");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn redacts_phone_numbers() {
        let result = redactor().redact("call +1 (415) 555-2671 tomorrow");
        assert_eq!(result.redacted_text, "call [REDACTED] tomorrow");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn redacts_credit_card_like_numbers() {
        let result = redactor().redact("card 4242 4242 4242 4242");
        assert_eq!(result.redacted_text, "card [REDACTED]");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn redacts_multiple_findings_in_one_text() {
        let result = redactor().redact("a@b.com and Bearer tok123");
        assert_eq!(result.redacted_text, "[REDACTED] and [REDACTED]");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn leaves_clean_text_untouched() {
        let result = redactor().redact("the quick brown fox");
        assert_eq!(result.redacted_text, "the quick brown fox");
        assert_eq!(result.status, RedactionStatus::Unredacted);
    }

    #[test]
    fn empty_redactor_passes_content_through() {
        let result = RegexRedactor::new().redact("a@b.com");
        assert_eq!(result.redacted_text, "a@b.com");
        assert_eq!(result.status, RedactionStatus::Unredacted);
    }

    #[test]
    fn supports_user_configured_patterns() {
        let mut redactor = RegexRedactor::new();
        redactor.add_pattern(Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());
        let result = redactor.redact("ssn: 123-45-6789");
        assert_eq!(result.redacted_text, "ssn: [REDACTED]");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }

    #[test]
    fn default_uses_builtin_patterns() {
        let result = RegexRedactor::default().redact("a@b.com");
        assert_eq!(result.status, RedactionStatus::Redacted);
    }
}
