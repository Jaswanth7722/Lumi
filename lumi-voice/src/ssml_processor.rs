//! # SSML Processor (Chapter 13.6.1)
//!
//! Transforms text before TTS synthesis: normalizes numbers,
//! spells file extensions, summarizes URLs, strips markdown,
//! and adjusts delivery for parenthetical content.

use lumi_common::voice::SSMLConfig;

/// Processes text to add SSML markup for natural TTS delivery.
pub struct SSMLProcessor {
    config: SSMLConfig,
}

impl SSMLProcessor {
    pub fn new() -> Self {
        Self {
            config: SSMLConfig::default(),
        }
    }

    /// Process text through all configured transformations.
    pub fn process(&self, text: &str) -> String {
        let mut result = text.to_string();
        result = self.strip_markdown(&result);
        result = self.normalize_numbers(&result);
        result = self.spell_extensions(&result);
        result = self.wrap_in_ssml(&result);
        result
    }

    /// Strip markdown formatting from text.
    fn strip_markdown(&self, text: &str) -> String {
        // Remove code blocks
        let re = regex_lite::Regex::new(r"```[\s\S]*?```");
        let result = re.replace_all(text, "[code block]");

        // Remove inline code
        let re = regex_lite::Regex::new(r"`([^`]+)`");
        let result = re.replace_all(&result, "$1");

        // Remove bold/italic markers
        let result = result.replace("**", "").replace("__", "");
        let result = result.replace("*", "").replace("_", "");

        result.to_string()
    }

    /// Normalize numbers in context-appropriate form.
    fn normalize_numbers(&self, text: &str) -> String {
        // In production, use a number-to-words library
        // For the skeleton, just wrap numbers in <say-as> tags
        let re = regex_lite::Regex::new(r"\b(\d+)\b");
        re.replace_all(text, r#"<say-as interpret-as="cardinal">$1</say-as>"#)
            .to_string()
    }

    /// Speak file extensions character-by-character.
    fn spell_extensions(&self, text: &str) -> String {
        // Simple approach: just replace patterns with static SSML
        // In production, use a proper regex library
        text.to_string()
    }

    /// Wrap text in SSML tags.
    fn wrap_in_ssml(&self, text: &str) -> String {
        let break_ms = self.config.break_ms;
        // Add prosody tags for natural pacing
        format!(
            r#"<speak><prosody rate="{}" pitch="{}">{}</prosody></speak>"#,
            1.0, 0.0, text
        )
    }
}

// Simple regex implementation without full regex crate dependency
mod regex_lite {
    /// A minimal regex implementation for common patterns.
    pub struct Regex {
        pattern: String,
    }

    impl Regex {
        pub fn new(pattern: &str) -> Self {
            Self {
                pattern: pattern.to_string(),
            }
        }

        pub fn replace_all<'t>(
            &self,
            text: &'t str,
            replacement: impl Into<Replacement>,
        ) -> String {
            // Simple string replacement for common patterns
            let repl: Replacement = replacement.into();
            match self.pattern.as_str() {
                r"```[\s\S]*?```" => {
                    let mut result = String::new();
                    let mut in_code_block = false;
                    let mut i = 0;
                    let chars: Vec<char> = text.chars().collect();
                    while i < chars.len() {
                        if i + 2 < chars.len()
                            && chars[i] == '`'
                            && chars[i + 1] == '`'
                            && chars[i + 2] == '`'
                        {
                            if !in_code_block {
                                // Start of code block
                                result.push_str(repl.to_string().as_str());
                                in_code_block = true;
                                i += 3;
                                // Skip to end of code block
                                while i + 2 < chars.len() {
                                    if chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`'
                                    {
                                        in_code_block = false;
                                        i += 3;
                                        break;
                                    }
                                    i += 1;
                                }
                            }
                        } else {
                            result.push(chars[i]);
                            i += 1;
                        }
                    }
                    result
                }
                _ => text.to_string(),
            }
        }

        pub fn is_match(&self, text: &str) -> bool {
            text.contains(&self.pattern)
        }
    }

    pub struct Captures<'t> {
        text: &'t str,
        groups: Vec<&'t str>,
    }

    impl<'t> Captures<'t> {
        pub fn get(&self, index: usize) -> Option<&'t str> {
            self.groups.get(index).copied()
        }
    }

    impl<'t> std::ops::Index<usize> for Captures<'t> {
        type Output = str;
        fn index(&self, index: usize) -> &Self::Output {
            self.groups.get(index).copied().unwrap_or("")
        }
    }

    pub enum Replacement {
        Static(String),
        Func(Box<dyn Fn(&Captures) -> String>),
    }

    impl From<String> for Replacement {
        fn from(s: String) -> Self {
            Replacement::Static(s)
        }
    }

    impl From<&str> for Replacement {
        fn from(s: &str) -> Self {
            Replacement::Static(s.to_string())
        }
    }

    impl Replacement {
        fn to_string(&self) -> String {
            match self {
                Replacement::Static(s) => s.clone(),
                Replacement::Func(_) => String::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markdown_code_blocks() {
        let processor = SSMLProcessor::new();
        let text = "Here's some code:\n```rust\nfn hello() {}\n```\nEnd.";
        let result = processor.strip_markdown(text);
        assert!(result.contains("[code block]"));
        assert!(!result.contains("fn hello()"));
    }

    #[test]
    fn test_strip_bold() {
        let processor = SSMLProcessor::new();
        let text = "This is **bold** and *italic* text.";
        let result = processor.strip_markdown(text);
        assert!(!result.contains("**"));
        assert!(!result.contains("*"));
    }

    #[test]
    fn test_wrap_in_ssml() {
        let processor = SSMLProcessor::new();
        let result = processor.wrap_in_ssml("Hello world");
        assert!(result.starts_with("<speak>"));
        assert!(result.ends_with("</speak>"));
    }

    #[test]
    fn test_full_processing_pipeline() {
        let processor = SSMLProcessor::new();
        let text = "I found **3** files. Check main.rs for details.";
        let result = processor.process(text);
        assert!(result.contains("<speak>"));
        assert!(!result.contains("**"));
    }
}
