// SPDX-License-Identifier: GPL-3.0-or-later

//! Remote Settings record parsing and the acceptance rule from
//! docs/model-compatibility.md: evaluate `filter_expression`, group records
//! into complete per-version model sets, keep versions inside the supported
//! major window, pick the newest set per pair.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::version::MozVersion;

/// Supported model major version window, a property of the vendored engine
/// pin (mozilla/translations 1de4a085d3a7 ⇒ wasm 4.0 ⇒ model major 3).
/// Mirrors Firefox's LANGUAGE_MODEL_MAJOR_VERSION_MIN/MAX; update together
/// with the pin and docs/model-compatibility.md.
pub const MODEL_MAJOR_MIN: u64 = 3;
pub const MODEL_MAJOR_MAX: u64 = 3;

/// One record of the `translations-models-v2` collection (one file).
#[derive(Debug, Clone, Deserialize)]
pub struct ModelRecord {
    pub id: String,
    pub name: String,
    #[serde(rename = "sourceLanguage")]
    pub source_language: String,
    #[serde(rename = "targetLanguage")]
    pub target_language: String,
    pub version: String,
    #[serde(rename = "fileType")]
    pub file_type: FileType,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub filter_expression: Option<String>,
    pub attachment: Attachment,
    #[serde(rename = "decompressedHash")]
    pub decompressed_hash: String,
    #[serde(rename = "decompressedSize")]
    pub decompressed_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Model,
    Lex,
    Vocab,
    SrcVocab,
    TrgVocab,
    /// Anything Mozilla adds later; carried but never required.
    #[serde(other)]
    Unknown,
}

/// `attachment` metadata: hash and size describe the compressed download.
#[derive(Debug, Clone, Deserialize)]
pub struct Attachment {
    pub hash: String,
    pub size: u64,
    pub location: String,
    #[serde(default)]
    pub mimetype: Option<String>,
}

/// The environment `filter_expression` is evaluated against.
#[derive(Debug, Clone)]
pub struct FilterEnv {
    /// `release`, or `nightly` when pre-release models are opted into.
    pub channel: &'static str,
    /// `Services.appinfo.OS`: `Linux` on desktop Linux.
    pub os: &'static str,
}

impl Default for FilterEnv {
    fn default() -> Self {
        FilterEnv {
            channel: "release",
            os: "Linux",
        }
    }
}

impl FilterEnv {
    pub fn prerelease(allow: bool) -> Self {
        FilterEnv {
            channel: if allow { "nightly" } else { "release" },
            ..FilterEnv::default()
        }
    }
}

/// Evaluates the tiny JEXL subset Mozilla actually publishes:
/// `env.channel`/`env.appinfo.OS`, `==`/`!=`, `'literal'`, `&&`, `||`.
/// Returns `None` for anything else, which callers treat as "skip the
/// record" (fail closed).
pub fn filter_matches(expression: Option<&str>, env: &FilterEnv) -> Option<bool> {
    let expression = expression.unwrap_or("").trim();
    if expression.is_empty() {
        return Some(true);
    }
    let mut any_clause = false;
    for clause in expression.split("||") {
        let mut all_terms = true;
        for term in clause.split("&&") {
            all_terms = all_terms && eval_term(term.trim(), env)?;
        }
        any_clause = any_clause || all_terms;
    }
    Some(any_clause)
}

fn eval_term(term: &str, env: &FilterEnv) -> Option<bool> {
    let (field, rest) = term
        .strip_prefix("env.channel")
        .map(|rest| (env.channel, rest))
        .or_else(|| {
            term.strip_prefix("env.appinfo.OS")
                .map(|rest| (env.os, rest))
        })?;
    let rest = rest.trim_start();
    let (negate, rest) = rest
        .strip_prefix("==")
        .map(|rest| (false, rest))
        .or_else(|| rest.strip_prefix("!=").map(|rest| (true, rest)))?;
    let literal = rest
        .trim()
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))?;
    if literal.contains('\'') {
        return None;
    }
    Some((field == literal) != negate)
}

/// A complete, accepted set of files for one pair at one version.
#[derive(Debug, Clone)]
pub struct ModelSet {
    pub source: String,
    pub target: String,
    pub variant: Option<String>,
    pub version: MozVersion,
    pub architecture: Option<String>,
    /// The per-file records, keyed by file type. Contains `model` plus
    /// either `vocab` or `srcvocab` + `trgvocab`, and possibly `lex`.
    pub files: BTreeMap<FileType, ModelRecord>,
}

impl ModelSet {
    pub fn pair(&self) -> String {
        format!("{}-{}", self.source, self.target)
    }
}

/// Applies the acceptance rule and returns the newest complete set per
/// (pair, variant). `skipped_filters` collects expressions we refused.
pub fn accepted_sets(
    records: &[ModelRecord],
    env: &FilterEnv,
    skipped_filters: &mut Vec<String>,
) -> Vec<ModelSet> {
    type GroupKey = (String, String, Option<String>, String);
    let mut groups: BTreeMap<GroupKey, Vec<&ModelRecord>> = BTreeMap::new();

    for record in records {
        match filter_matches(record.filter_expression.as_deref(), env) {
            Some(true) => {}
            Some(false) => continue,
            None => {
                let expression = record.filter_expression.clone().unwrap_or_default();
                if !skipped_filters.contains(&expression) {
                    skipped_filters.push(expression);
                }
                continue;
            }
        }
        let Ok(version) = MozVersion::parse(&record.version) else {
            continue;
        };
        if version.major() < MODEL_MAJOR_MIN || version.major() > MODEL_MAJOR_MAX {
            continue;
        }
        groups
            .entry((
                record.source_language.clone(),
                record.target_language.clone(),
                record.variant.clone(),
                record.version.clone(),
            ))
            .or_default()
            .push(record);
    }

    let mut best: BTreeMap<(String, String, Option<String>), ModelSet> = BTreeMap::new();
    for ((source, target, variant, version), group) in groups {
        let mut files = BTreeMap::new();
        for record in group {
            files.insert(record.file_type, record.clone());
        }
        let complete = files.contains_key(&FileType::Model)
            && (files.contains_key(&FileType::Vocab)
                || (files.contains_key(&FileType::SrcVocab)
                    && files.contains_key(&FileType::TrgVocab)));
        if !complete {
            continue;
        }
        let version = MozVersion::parse(&version).expect("checked above");
        let architecture = files
            .get(&FileType::Model)
            .and_then(|r| r.architecture.clone());
        let set = ModelSet {
            source: source.clone(),
            target: target.clone(),
            variant: variant.clone(),
            version,
            architecture,
            files,
        };
        let key = (source, target, variant);
        match best.get(&key) {
            Some(existing) if existing.version >= set.version => {}
            _ => {
                best.insert(key, set);
            }
        }
    }
    best.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(src: &str, trg: &str, version: &str, file_type: &str, filter: &str) -> ModelRecord {
        serde_json::from_value(serde_json::json!({
            "id": format!("{src}-{trg}-{version}-{file_type}"),
            "name": format!("{file_type}.{src}{trg}.bin"),
            "sourceLanguage": src,
            "targetLanguage": trg,
            "version": version,
            "fileType": file_type,
            "filter_expression": filter,
            "attachment": {"hash": "h", "size": 1, "location": "l"},
            "decompressedHash": "d",
            "decompressedSize": 2,
        }))
        .unwrap()
    }

    fn set_for<'a>(sets: &'a [ModelSet], pair: &str) -> Option<&'a ModelSet> {
        sets.iter().find(|s| s.pair() == pair)
    }

    #[test]
    fn filter_subset() {
        let env = FilterEnv::default();
        assert_eq!(filter_matches(None, &env), Some(true));
        assert_eq!(filter_matches(Some(""), &env), Some(true));
        assert_eq!(
            filter_matches(Some("env.appinfo.OS == 'Android'"), &env),
            Some(false)
        );
        assert_eq!(
            filter_matches(Some("env.appinfo.OS != 'Android'"), &env),
            Some(true)
        );
        assert_eq!(
            filter_matches(
                Some("env.channel == 'default' || env.channel == 'nightly'"),
                &env
            ),
            Some(false)
        );
        assert_eq!(
            filter_matches(
                Some("env.channel == 'default' || env.channel == 'nightly'"),
                &FilterEnv::prerelease(true)
            ),
            Some(true)
        );
        assert_eq!(
            filter_matches(
                Some("env.appinfo.OS != 'Android' || env.channel != 'release'"),
                &env
            ),
            Some(true)
        );
        // Unknown syntax fails closed.
        assert_eq!(filter_matches(Some("env.version >= '144'"), &env), None);
        assert_eq!(filter_matches(Some("1 == 1"), &env), None);
    }

    #[test]
    fn picks_newest_complete_set_in_window() {
        let records = vec![
            record("bg", "en", "3.0", "model", ""),
            record("bg", "en", "3.0", "vocab", ""),
            record("bg", "en", "3.0", "lex", ""),
            // Newer but incomplete: no vocab.
            record("bg", "en", "3.2", "model", ""),
            // Newer complete 3.1.
            record("bg", "en", "3.1", "model", ""),
            record("bg", "en", "3.1", "vocab", ""),
            // Outside the window.
            record("bg", "en", "2.0", "model", ""),
            record("bg", "en", "2.0", "vocab", ""),
            record("bg", "en", "4.0", "model", ""),
            record("bg", "en", "4.0", "vocab", ""),
        ];
        let mut skipped = Vec::new();
        let sets = accepted_sets(&records, &FilterEnv::default(), &mut skipped);
        let set = set_for(&sets, "bg-en").unwrap();
        assert_eq!(set.version.as_str(), "3.1");
        assert!(skipped.is_empty());
    }

    #[test]
    fn prerelease_gating_and_split_vocab() {
        let nightly = "env.channel == 'default' || env.channel == 'nightly'";
        let records = vec![
            record("ja", "en", "3.0", "model", ""),
            record("ja", "en", "3.0", "srcvocab", ""),
            record("ja", "en", "3.0", "trgvocab", ""),
            record("ja", "en", "3.1a1", "model", nightly),
            record("ja", "en", "3.1a1", "srcvocab", nightly),
            record("ja", "en", "3.1a1", "trgvocab", nightly),
        ];
        let mut skipped = Vec::new();
        let stable = accepted_sets(&records, &FilterEnv::default(), &mut skipped);
        assert_eq!(set_for(&stable, "ja-en").unwrap().version.as_str(), "3.0");
        let pre = accepted_sets(&records, &FilterEnv::prerelease(true), &mut skipped);
        assert_eq!(set_for(&pre, "ja-en").unwrap().version.as_str(), "3.1a1");
    }

    #[test]
    fn unknown_filters_are_reported_and_skipped() {
        let records = vec![
            record("de", "en", "3.0", "model", "env.mystery == 'x'"),
            record("de", "en", "3.0", "vocab", ""),
        ];
        let mut skipped = Vec::new();
        let sets = accepted_sets(&records, &FilterEnv::default(), &mut skipped);
        assert!(
            set_for(&sets, "de-en").is_none(),
            "incomplete without model"
        );
        assert_eq!(skipped, vec!["env.mystery == 'x'".to_owned()]);
    }
}
