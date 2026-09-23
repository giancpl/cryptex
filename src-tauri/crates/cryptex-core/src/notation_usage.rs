//! Conservative recognition of explicitly declared notation forms.

use crate::{
    api::API_VERSION,
    index::{FileIndexStatus, IndexSourceRange, ProjectIndex, ScannerLimits},
    notation::EffectiveNotationProfile,
    project::ProjectPath,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use ts_rs::TS;

pub const NOTATION_USAGE_SCHEMA_VERSION: u16 = 1;
const VERBATIM_ENVIRONMENTS: &[&str] = &["verbatim", "verbatim*", "lstlisting", "minted"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationUsage {
    pub concept_id: String,
    pub form: String,
    pub preferred: bool,
    pub range: IndexSourceRange,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationFileUsage {
    pub relative_path: String,
    pub fingerprint: String,
    pub usages: Vec<NotationUsage>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectNotationUsage {
    pub api_version: u16,
    pub schema_version: u16,
    pub profile_version: u16,
    pub project_id: String,
    pub index_generation: u64,
    pub files: Vec<NotationFileUsage>,
    pub incomplete: bool,
}

#[derive(Clone, Debug)]
struct DeclaredForm {
    concept_id: String,
    form: String,
    preferred: bool,
}

#[derive(Default)]
struct TrieNode {
    children: HashMap<u8, usize>,
    form: Option<usize>,
}

struct FormMatcher {
    forms: Vec<DeclaredForm>,
    nodes: Vec<TrieNode>,
}

impl FormMatcher {
    fn from_profile(profile: &EffectiveNotationProfile) -> Self {
        let mut forms = Vec::new();
        for concept in &profile.concepts {
            for form in &concept.declared_forms {
                // Plain identifiers and punctuation are too ambiguous for a
                // high-confidence lexical classification.
                if !form.starts_with('\\') || form.len() < 2 {
                    continue;
                }
                forms.push(DeclaredForm {
                    concept_id: concept.id.clone(),
                    form: form.clone(),
                    preferred: form == &concept.preferred_form,
                });
            }
        }
        forms.sort_by(|left, right| {
            left.form
                .cmp(&right.form)
                .then_with(|| left.concept_id.cmp(&right.concept_id))
        });
        let mut matcher = Self {
            forms,
            nodes: vec![TrieNode::default()],
        };
        for index in 0..matcher.forms.len() {
            let bytes = matcher.forms[index].form.as_bytes().to_vec();
            let mut node = 0;
            for byte in bytes {
                let next = if let Some(next) = matcher.nodes[node].children.get(&byte) {
                    *next
                } else {
                    let next = matcher.nodes.len();
                    matcher.nodes.push(TrieNode::default());
                    matcher.nodes[node].children.insert(byte, next);
                    next
                };
                node = next;
            }
            matcher.nodes[node].form = Some(index);
        }
        matcher
    }

    fn longest_match(&self, source: &str, start: usize) -> Option<&DeclaredForm> {
        let bytes = source.as_bytes();
        let mut node = 0;
        let mut cursor = start;
        let mut found = None;
        while let Some(byte) = bytes.get(cursor) {
            let Some(next) = self.nodes[node].children.get(byte) else {
                break;
            };
            node = *next;
            cursor += 1;
            if let Some(index) = self.nodes[node].form
                && form_boundary(source, cursor, matcher_last_byte(&self.forms[index].form))
            {
                found = Some(index);
            }
        }
        found.map(|index| &self.forms[index])
    }
}

pub fn scan_notation_file(
    relative_path: &str,
    fingerprint: &str,
    source: &str,
    profile: &EffectiveNotationProfile,
    limits: ScannerLimits,
) -> Result<NotationFileUsage, NotationScanError> {
    let matcher = FormMatcher::from_profile(profile);
    scan_notation_file_with_matcher(relative_path, fingerprint, source, &matcher, limits)
}

fn scan_notation_file_with_matcher(
    relative_path: &str,
    fingerprint: &str,
    source: &str,
    matcher: &FormMatcher,
    limits: ScannerLimits,
) -> Result<NotationFileUsage, NotationScanError> {
    if relative_path.is_empty() || ProjectPath::parse(relative_path).is_err() {
        return Err(NotationScanError::InvalidRelativePath);
    }
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(NotationScanError::InvalidFingerprint);
    }
    if source.len() as u64 > limits.max_file_bytes {
        return Ok(NotationFileUsage {
            relative_path: relative_path.to_owned(),
            fingerprint: fingerprint.to_owned(),
            usages: Vec::new(),
            truncated: true,
        });
    }
    let mut scanner = UsageScanner::new(source, matcher, limits.max_records_per_file as usize);
    scanner.scan();
    Ok(NotationFileUsage {
        relative_path: relative_path.to_owned(),
        fingerprint: fingerprint.to_owned(),
        usages: scanner.usages,
        truncated: scanner.truncated,
    })
}

pub fn scan_project_notation_usage<F>(
    index: &ProjectIndex,
    profile: &EffectiveNotationProfile,
    mut read: F,
) -> ProjectNotationUsage
where
    F: FnMut(&str) -> Option<(String, String)>,
{
    let mut files = Vec::new();
    let mut incomplete = !index.issues.is_empty();
    let mut total_bytes = 0_u64;
    let matcher = FormMatcher::from_profile(profile);
    for indexed in &index.files {
        if indexed.status == FileIndexStatus::Skipped {
            incomplete = true;
            continue;
        }
        let Some((source, fingerprint)) = read(&indexed.relative_path) else {
            incomplete = true;
            continue;
        };
        if fingerprint != indexed.fingerprint {
            incomplete = true;
            continue;
        }
        total_bytes = total_bytes.saturating_add(source.len() as u64);
        if total_bytes > index.scanner_limits.max_total_bytes {
            incomplete = true;
            break;
        }
        match scan_notation_file_with_matcher(
            &indexed.relative_path,
            &fingerprint,
            &source,
            &matcher,
            index.scanner_limits,
        ) {
            Ok(file) => {
                incomplete |= file.truncated;
                files.push(file);
            }
            Err(_) => incomplete = true,
        }
    }
    ProjectNotationUsage {
        api_version: API_VERSION,
        schema_version: NOTATION_USAGE_SCHEMA_VERSION,
        profile_version: profile.profile_version,
        project_id: index.project_id.clone(),
        index_generation: index.generation,
        files,
        incomplete,
    }
}

struct UsageScanner<'a> {
    source: &'a str,
    bytes: &'a [u8],
    matcher: &'a FormMatcher,
    max_records: usize,
    position: usize,
    line: u32,
    column: u32,
    usages: Vec<NotationUsage>,
    truncated: bool,
}

impl<'a> UsageScanner<'a> {
    fn new(source: &'a str, matcher: &'a FormMatcher, max_records: usize) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            matcher,
            max_records,
            position: 0,
            line: 1,
            column: 1,
            usages: Vec::new(),
            truncated: false,
        }
    }

    fn scan(&mut self) {
        while self.position < self.bytes.len() {
            if self.bytes[self.position] == b'%' && !is_escaped(self.bytes, self.position) {
                let end = self.bytes[self.position..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(self.bytes.len(), |offset| self.position + offset + 1);
                self.advance_to(end);
                continue;
            }
            if self.bytes[self.position] == b'\\' && is_escaped(self.bytes, self.position) {
                self.advance_char();
                continue;
            }
            if let Some(end) = inline_verb_end(self.source, self.position) {
                self.advance_to(end);
                continue;
            }
            if let Some(end) = verbatim_environment_end(self.source, self.position) {
                self.advance_to(end);
                continue;
            }
            if self.bytes[self.position] == b'\\'
                && let Some(form) = self
                    .matcher
                    .longest_match(self.source, self.position)
                    .cloned()
            {
                if self.usages.len() >= self.max_records {
                    self.truncated = true;
                    break;
                }
                let start = self.position;
                let start_line = self.line;
                let start_column = self.column;
                let end = start + form.form.len();
                self.advance_to(end);
                self.usages.push(NotationUsage {
                    concept_id: form.concept_id,
                    form: form.form,
                    preferred: form.preferred,
                    range: IndexSourceRange {
                        start_byte: start as u64,
                        end_byte: end as u64,
                        start_line,
                        start_column,
                        end_line: self.line,
                        end_column: self.column,
                    },
                });
                continue;
            }
            self.advance_char();
        }
    }

    fn advance_char(&mut self) {
        let character = self.source[self.position..]
            .chars()
            .next()
            .expect("position is a character boundary");
        self.position += character.len_utf8();
        if character == '\n' {
            self.line = self.line.saturating_add(1);
            self.column = 1;
        } else {
            self.column = self.column.saturating_add(1);
        }
    }

    fn advance_to(&mut self, end: usize) {
        while self.position < end {
            self.advance_char();
        }
    }
}

fn matcher_last_byte(form: &str) -> u8 {
    *form.as_bytes().last().expect("declared forms are nonempty")
}

fn form_boundary(source: &str, end: usize, last: u8) -> bool {
    (!last.is_ascii_alphabetic() && last != b'@')
        || !source[end..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '@')
}

fn is_escaped(bytes: &[u8], position: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = position;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

fn inline_verb_end(source: &str, start: usize) -> Option<usize> {
    let tail = source.get(start..)?;
    let prefix = if tail.starts_with("\\verb*") {
        "\\verb*"
    } else if tail.starts_with("\\verb") {
        "\\verb"
    } else {
        return None;
    };
    let delimiter_position = start + prefix.len();
    let delimiter = *source.as_bytes().get(delimiter_position)?;
    if prefix == "\\verb" && (delimiter.is_ascii_alphabetic() || delimiter == b'@') {
        return None;
    }
    if delimiter.is_ascii_whitespace() {
        return None;
    }
    if !delimiter.is_ascii() {
        return Some(source.len());
    }
    source.as_bytes()[delimiter_position + 1..]
        .iter()
        .position(|byte| *byte == delimiter)
        .map_or(Some(source.len()), |offset| {
            Some(delimiter_position + offset + 2)
        })
}

fn verbatim_environment_end(source: &str, start: usize) -> Option<usize> {
    for environment in VERBATIM_ENVIRONMENTS {
        let opening = format!("\\begin{{{environment}}}");
        if source[start..].starts_with(&opening) {
            let content = start + opening.len();
            let closing = format!("\\end{{{environment}}}");
            return Some(
                source[content..]
                    .find(&closing)
                    .map_or(source.len(), |offset| content + offset + closing.len()),
            );
        }
    }
    None
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum NotationScanError {
    #[error("notation scanner path is not a valid nonempty project-relative path")]
    InvalidRelativePath,
    #[error("notation scanner fingerprint is not a lowercase SHA-256 digest")]
    InvalidFingerprint,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::{EffectiveNotationConcept, NotationPreferenceSource};
    use std::collections::BTreeMap;

    fn profile(forms: &[(&str, &str, bool)]) -> EffectiveNotationProfile {
        let mut concepts: BTreeMap<&str, Vec<(&str, bool)>> = BTreeMap::new();
        for (concept, form, preferred) in forms {
            concepts
                .entry(concept)
                .or_default()
                .push((form, *preferred));
        }
        EffectiveNotationProfile {
            api_version: 1,
            profile_version: 1,
            name: "test".to_owned(),
            project_id: None,
            concepts: concepts
                .into_iter()
                .map(|(id, forms)| EffectiveNotationConcept {
                    id: id.to_owned(),
                    label: id.to_owned(),
                    preferred_form: forms
                        .iter()
                        .find(|(_, preferred)| *preferred)
                        .map_or(forms[0].0, |(form, _)| *form)
                        .to_owned(),
                    declared_forms: forms.iter().map(|(form, _)| (*form).to_owned()).collect(),
                    source: NotationPreferenceSource::Default,
                })
                .collect(),
        }
    }

    #[test]
    fn finds_only_declared_exact_command_forms_with_ranges() {
        let source = "α \\mathcal{A} and \\Pr but not \\Prime or A";
        let result = scan_notation_file(
            "main.tex",
            &"a".repeat(64),
            source,
            &profile(&[
                ("adversary", "\\mathcal{A}", true),
                ("probability", "\\mathbb{P}", true),
                ("probability", "\\Pr", false),
                ("plain", "A", true),
            ]),
            ScannerLimits::default(),
        )
        .unwrap();
        assert_eq!(result.usages.len(), 2);
        assert_eq!(result.usages[0].concept_id, "adversary");
        assert_eq!(result.usages[0].range.start_column, 3);
        assert_eq!(result.usages[1].form, "\\Pr");
        assert!(!result.usages[1].preferred);
    }

    #[test]
    fn skips_comments_inline_verb_and_verbatim_environments() {
        let source = "\\Pr % \\Pr\n\\verb|\\Pr| \\verb*+\\Pr+\n\\begin{verbatim}\n\\Pr\n\\end{verbatim}\n\\Pr";
        let result = scan_notation_file(
            "main.tex",
            &"b".repeat(64),
            source,
            &profile(&[("probability", "\\Pr", true)]),
            ScannerLimits::default(),
        )
        .unwrap();
        assert_eq!(result.usages.len(), 2);
        assert_eq!(result.usages[0].range.start_line, 1);
        assert_eq!(result.usages[1].range.start_line, 6);
    }

    #[test]
    fn escaped_commands_and_longer_verb_macros_are_not_misclassified() {
        let source = "\\\\Pr \\verbatimtoken \\Pr";
        let result = scan_notation_file(
            "main.tex",
            &"9".repeat(64),
            source,
            &profile(&[("probability", "\\Pr", true)]),
            ScannerLimits::default(),
        )
        .unwrap();
        assert_eq!(result.usages.len(), 1);
        assert_eq!(result.usages[0].range.start_column, 21);
    }

    #[test]
    fn escaped_percent_does_not_hide_later_usage() {
        let result = scan_notation_file(
            "main.tex",
            &"c".repeat(64),
            "cost \\% then \\Pr",
            &profile(&[("probability", "\\Pr", true)]),
            ScannerLimits::default(),
        )
        .unwrap();
        assert_eq!(result.usages.len(), 1);
    }

    #[test]
    fn enforces_file_and_record_limits() {
        let profile = profile(&[("probability", "\\Pr", true)]);
        let mut limits = ScannerLimits {
            max_records_per_file: 2,
            ..ScannerLimits::default()
        };
        let result = scan_notation_file(
            "main.tex",
            &"d".repeat(64),
            "\\Pr \\Pr \\Pr",
            &profile,
            limits,
        )
        .unwrap();
        assert_eq!(result.usages.len(), 2);
        assert!(result.truncated);
        limits.max_file_bytes = 2;
        assert!(
            scan_notation_file("main.tex", &"d".repeat(64), "\\Pr", &profile, limits)
                .unwrap()
                .truncated
        );
    }

    #[test]
    fn project_scan_rejects_stale_fingerprints_and_marks_partial_evidence() {
        let fingerprint = "e".repeat(64);
        let index = ProjectIndex {
            api_version: 1,
            schema_version: 1,
            project_id: "f".repeat(64),
            generation: 3,
            completeness: crate::index::IndexCompleteness::BestEffort,
            scanner_limits: ScannerLimits::default(),
            files: vec![crate::index::IndexedFile {
                relative_path: "main.tex".to_owned(),
                fingerprint: fingerprint.clone(),
                status: FileIndexStatus::Complete,
                records: Vec::new(),
                issues: Vec::new(),
            }],
            issues: Vec::new(),
        };
        let result =
            scan_project_notation_usage(&index, &profile(&[("probability", "\\Pr", true)]), |_| {
                Some(("\\Pr".to_owned(), "0".repeat(64)))
            });
        assert!(result.incomplete);
        assert!(result.files.is_empty());
        assert_eq!(result.index_generation, 3);
    }
}
