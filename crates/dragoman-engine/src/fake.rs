// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A fake [`Backend`] for tests: deterministic output, configurable delays
//! and failure injection, no C++ and no model files.

use std::time::Duration;

use crate::backend::{
    Backend, Error, ModelFiles, Result, SentencePair, TranslateOptions, Translation,
};

/// Configuration for [`FakeBackend`]. The defaults succeed instantly.
#[derive(Debug, Clone, Default)]
pub struct FakeConfig {
    pub load_delay: Duration,
    pub translate_delay: Duration,
    /// When set, every load fails with this message.
    pub fail_load: Option<String>,
    /// When set, every translation fails with this message.
    pub fail_translate: Option<String>,
}

pub struct FakeBackend {
    config: FakeConfig,
}

/// The fake's "model": tagged with the model file's stem, so outputs show
/// which model (or pivot chain) produced them.
pub struct FakeModel {
    tag: String,
}

impl FakeBackend {
    pub fn new(config: FakeConfig) -> Self {
        FakeBackend { config }
    }
}

impl Backend for FakeBackend {
    type Model = FakeModel;

    fn load(&mut self, files: &ModelFiles, _config_yaml: &str) -> Result<FakeModel> {
        std::thread::sleep(self.config.load_delay);
        if let Some(message) = &self.config.fail_load {
            return Err(Error::Load(message.clone()));
        }
        let tag = files
            .model
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "model".to_owned());
        Ok(FakeModel { tag })
    }

    fn translate(
        &mut self,
        first: &FakeModel,
        second: Option<&FakeModel>,
        segments: Vec<String>,
        options: TranslateOptions,
    ) -> Result<Vec<Translation>> {
        std::thread::sleep(self.config.translate_delay);
        if let Some(message) = &self.config.fail_translate {
            return Err(Error::Engine(message.clone()));
        }
        let tag = match second {
            Some(second) => format!("{}+{}", first.tag, second.tag),
            None => first.tag.clone(),
        };
        let html = if options.html { ",html" } else { "" };
        Ok(segments
            .into_iter()
            .map(|s| {
                let prefix = format!("[{tag}{html}] ");
                // The output is the input behind a prefix, so every source
                // sentence maps to the same bytes shifted by the prefix; the
                // first one also covers the prefix.
                let sentences = split_sentences(&s)
                    .into_iter()
                    .enumerate()
                    .map(|(i, range)| SentencePair {
                        target: if i == 0 {
                            0
                        } else {
                            range.start + prefix.len()
                        }..range.end + prefix.len(),
                        source: range,
                    })
                    .collect();
                Translation {
                    text: prefix + &s,
                    sentences,
                }
            })
            .collect())
    }
}

/// Byte ranges of the sentences in `text`: a sentence ends after `.`, `!` or
/// `?` followed by whitespace, which then belongs to the gap before the next
/// one. Text without letters or digits has no sentences.
pub fn split_sentences(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut sentences = Vec::new();
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if start.is_none() && !c.is_whitespace() {
            start = Some(i);
        }
        let ends = matches!(c, '.' | '!' | '?')
            && chars.peek().is_none_or(|(_, next)| next.is_whitespace());
        if ends {
            if let Some(begin) = start.take() {
                sentences.push(begin..i + c.len_utf8());
            }
        }
    }
    if let Some(begin) = start {
        sentences.push(begin..text.trim_end().len());
    }
    sentences.retain(|r| text[r.clone()].chars().any(char::is_alphanumeric));
    sentences
}

#[cfg(test)]
mod tests {
    use super::split_sentences;

    #[test]
    fn splits_sentences() {
        let text = "Добро утро. How are you?  Fine!";
        let parts: Vec<&str> = split_sentences(text)
            .into_iter()
            .map(|r| &text[r])
            .collect();
        assert_eq!(parts, ["Добро утро.", "How are you?", "Fine!"]);
        assert!(split_sentences("  ...  ").is_empty());
        assert_eq!(split_sentences("no end "), vec![0..6]);
    }
}
