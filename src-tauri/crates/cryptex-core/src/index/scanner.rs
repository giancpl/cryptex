use super::{
    FileIndexStatus, IndexConfidence, IndexIssue, IndexIssueCode, IndexProvenance, IndexRecord,
    IndexRecordKind, IndexSourceRange, IndexedFile, ScannerLimits,
};
use crate::project::ProjectPath;
use thiserror::Error;

const VERBATIM_ENVIRONMENTS: &[&str] = &["verbatim", "verbatim*", "lstlisting", "minted"];
const SECTION_COMMANDS: &[&str] = &[
    "part",
    "chapter",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "subparagraph",
];
const REFERENCE_COMMANDS: &[&str] = &["ref", "pageref", "eqref", "autoref", "cref", "Cref"];
const CITATION_COMMANDS: &[&str] = &[
    "cite",
    "citep",
    "citet",
    "parencite",
    "textcite",
    "autocite",
];
const MACRO_DEFINITION_COMMANDS: &[&str] = &[
    "newcommand",
    "renewcommand",
    "providecommand",
    "DeclareRobustCommand",
];
const INCLUDE_COMMANDS: &[&str] = &["input", "include", "subfile"];

pub fn scan_latex_file(
    relative_path: &str,
    fingerprint: &str,
    source: &str,
    limits: ScannerLimits,
) -> Result<IndexedFile, ScanError> {
    if ProjectPath::parse(relative_path).is_err() || relative_path.is_empty() {
        return Err(ScanError::InvalidRelativePath);
    }
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ScanError::InvalidFingerprint);
    }
    if source.len() as u64 > limits.max_file_bytes {
        return Ok(skipped(
            relative_path,
            fingerprint,
            IndexIssueCode::FileTooLarge,
            "file exceeds the configured indexing byte limit",
        ));
    }

    Ok(Scanner::new(relative_path, fingerprint, source, limits).scan())
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ScanError {
    #[error("scanner path is not a valid nonempty project-relative path")]
    InvalidRelativePath,
    #[error("scanner fingerprint is not a lowercase SHA-256 digest")]
    InvalidFingerprint,
}

fn skipped(
    relative_path: &str,
    fingerprint: &str,
    code: IndexIssueCode,
    message: &str,
) -> IndexedFile {
    IndexedFile {
        relative_path: relative_path.to_owned(),
        fingerprint: fingerprint.to_owned(),
        status: FileIndexStatus::Skipped,
        records: Vec::new(),
        issues: vec![IndexIssue {
            code,
            message: message.to_owned(),
            relative_path: Some(relative_path.to_owned()),
            range: None,
        }],
    }
}

struct Scanner<'a> {
    path: &'a str,
    fingerprint: &'a str,
    source: &'a str,
    bytes: &'a [u8],
    limits: ScannerLimits,
    position: usize,
    line_starts: Vec<usize>,
    records: Vec<IndexRecord>,
    issues: Vec<IndexIssue>,
    partial: bool,
    record_limit_reported: bool,
}

impl<'a> Scanner<'a> {
    fn new(path: &'a str, fingerprint: &'a str, source: &'a str, limits: ScannerLimits) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        Self {
            path,
            fingerprint,
            source,
            bytes: source.as_bytes(),
            limits,
            position: 0,
            line_starts,
            records: Vec::new(),
            issues: Vec::new(),
            partial: false,
            record_limit_reported: false,
        }
    }

    fn scan(mut self) -> IndexedFile {
        while self.position < self.bytes.len() {
            match self.bytes[self.position] {
                b'%' => self.skip_comment(),
                b'\\' => self.scan_command(),
                _ => self.position += 1,
            }
        }
        IndexedFile {
            relative_path: self.path.to_owned(),
            fingerprint: self.fingerprint.to_owned(),
            status: if self.partial {
                FileIndexStatus::Partial
            } else {
                FileIndexStatus::Complete
            },
            records: self.records,
            issues: self.issues,
        }
    }

    fn skip_comment(&mut self) {
        self.position = self.bytes[self.position..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(self.bytes.len(), |offset| self.position + offset + 1);
    }

    fn scan_command(&mut self) {
        let start = self.position;
        let Some((name, mut cursor)) = self.command_name(start) else {
            self.position += 1;
            return;
        };
        if cursor - start > self.limits.max_command_bytes as usize {
            self.issue(
                IndexIssueCode::CommandTooLong,
                "command name exceeds the configured byte limit",
                start,
                cursor,
            );
            self.position = cursor;
            return;
        }
        if name == "verb" {
            if self.bytes.get(cursor) == Some(&b'*') {
                cursor += 1;
            }
            self.position = self.skip_inline_verb(cursor);
            return;
        }
        if self.bytes.get(cursor) == Some(&b'*') {
            cursor += 1;
        }
        cursor = self.skip_space(cursor);

        if name == "usepackage" || name == "RequirePackage" {
            cursor = self.skip_optional_argument(cursor);
        }

        let needs_argument = SECTION_COMMANDS.contains(&name)
            || REFERENCE_COMMANDS.contains(&name)
            || CITATION_COMMANDS.contains(&name)
            || MACRO_DEFINITION_COMMANDS.contains(&name)
            || INCLUDE_COMMANDS.contains(&name)
            || matches!(
                name,
                "label" | "begin" | "usepackage" | "RequirePackage" | "pseudocode"
            );
        if !needs_argument {
            self.position = cursor.max(start + 1);
            return;
        }

        let Some(group) = self.parse_group(cursor) else {
            self.issue(
                IndexIssueCode::MalformedInput,
                "command argument is missing or unbalanced",
                start,
                cursor.max(start + 1),
            );
            self.position = cursor.max(start + 1);
            return;
        };
        let value = self.source[group.content_start..group.content_end].trim();
        let end = group.end;

        if !value.is_empty() {
            if SECTION_COMMANDS.contains(&name) {
                self.record(IndexRecordKind::Section, value, None, start, end);
            } else if name == "label" {
                self.record(IndexRecordKind::Label, value, None, start, end);
            } else if REFERENCE_COMMANDS.contains(&name) {
                self.record(IndexRecordKind::Reference, value, None, start, end);
            } else if CITATION_COMMANDS.contains(&name) {
                for citation in value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                {
                    self.record(IndexRecordKind::Citation, citation, None, start, end);
                }
            } else if INCLUDE_COMMANDS.contains(&name) {
                self.record(IndexRecordKind::Include, value, None, start, end);
            } else if matches!(name, "usepackage" | "RequirePackage") {
                for package in value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                {
                    self.record(IndexRecordKind::Package, package, None, start, end);
                }
            } else if MACRO_DEFINITION_COMMANDS.contains(&name) {
                let macro_name = value.trim_start_matches('\\').trim();
                if !macro_name.is_empty() {
                    self.record(
                        IndexRecordKind::MacroDefinition,
                        macro_name,
                        None,
                        start,
                        end,
                    );
                }
            } else if name == "begin" {
                self.record(IndexRecordKind::Environment, value, None, start, end);
                if VERBATIM_ENVIRONMENTS.contains(&value) {
                    self.position = self.skip_verbatim_environment(value, end);
                    return;
                }
            } else if name == "pseudocode" {
                self.record(IndexRecordKind::Cryptocode, name, None, start, end);
            }
        }
        self.position = end;
    }

    fn command_name(&self, start: usize) -> Option<(&'a str, usize)> {
        let first = *self.bytes.get(start + 1)?;
        if !first.is_ascii_alphabetic() && first != b'@' {
            return first
                .is_ascii()
                .then(|| (&self.source[start + 1..start + 2], start + 2));
        }
        let mut end = start + 1;
        while self
            .bytes
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'@')
        {
            end += 1;
        }
        Some((&self.source[start + 1..end], end))
    }

    fn skip_inline_verb(&self, cursor: usize) -> usize {
        let Some(&delimiter) = self.bytes.get(cursor) else {
            return cursor;
        };
        if delimiter == b'\n' || delimiter.is_ascii_whitespace() {
            return cursor + 1;
        }
        self.bytes[cursor + 1..]
            .iter()
            .position(|byte| *byte == delimiter)
            .map_or(self.bytes.len(), |offset| cursor + offset + 2)
    }

    fn skip_verbatim_environment(&self, environment: &str, cursor: usize) -> usize {
        let terminator = format!("\\end{{{environment}}}");
        self.source[cursor..]
            .find(&terminator)
            .map_or(self.bytes.len(), |offset| {
                cursor + offset + terminator.len()
            })
    }

    fn skip_space(&self, mut cursor: usize) -> usize {
        while self
            .bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor += 1;
        }
        cursor
    }

    fn skip_optional_argument(&mut self, cursor: usize) -> usize {
        if self.bytes.get(cursor) != Some(&b'[') {
            return cursor;
        }
        let mut depth = 1_u16;
        let mut current = cursor + 1;
        while current < self.bytes.len() {
            match self.bytes[current] {
                b'\\' => current = (current + 2).min(self.bytes.len()),
                b'[' => {
                    depth = depth.saturating_add(1);
                    if depth > self.limits.max_brace_depth {
                        self.issue(
                            IndexIssueCode::BraceDepthLimitReached,
                            "optional argument nesting exceeds the configured limit",
                            cursor,
                            current + 1,
                        );
                        return current + 1;
                    }
                    current += 1;
                }
                b']' => {
                    depth -= 1;
                    current += 1;
                    if depth == 0 {
                        return self.skip_space(current);
                    }
                }
                _ => current += 1,
            }
        }
        self.issue(
            IndexIssueCode::MalformedInput,
            "optional argument is unbalanced",
            cursor,
            self.bytes.len(),
        );
        self.bytes.len()
    }

    fn parse_group(&mut self, cursor: usize) -> Option<Group> {
        if self.bytes.get(cursor) != Some(&b'{') {
            return None;
        }
        let mut depth = 1_u16;
        let mut current = cursor + 1;
        while current < self.bytes.len() {
            match self.bytes[current] {
                b'\\' => current = (current + 2).min(self.bytes.len()),
                b'%' => {
                    current = self.bytes[current..]
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .map_or(self.bytes.len(), |offset| current + offset + 1);
                }
                b'{' => {
                    depth = depth.saturating_add(1);
                    if depth > self.limits.max_brace_depth {
                        self.issue(
                            IndexIssueCode::BraceDepthLimitReached,
                            "brace nesting exceeds the configured limit",
                            cursor,
                            current + 1,
                        );
                        return None;
                    }
                    current += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Group {
                            content_start: cursor + 1,
                            content_end: current,
                            end: current + 1,
                        });
                    }
                    current += 1;
                }
                _ => current += 1,
            }
        }
        None
    }

    fn record(
        &mut self,
        kind: IndexRecordKind,
        name: &str,
        target: Option<String>,
        start: usize,
        end: usize,
    ) {
        if self.records.len() >= self.limits.max_records_per_file as usize {
            if !self.record_limit_reported {
                self.issue(
                    IndexIssueCode::RecordLimitReached,
                    "record count reached the configured per-file limit",
                    start,
                    end,
                );
                self.record_limit_reported = true;
            }
            return;
        }
        self.records.push(IndexRecord {
            kind,
            name: name.to_owned(),
            target,
            range: self.range(start, end),
            confidence: IndexConfidence::Exact,
            provenance: IndexProvenance::Lexical,
        });
    }

    fn issue(&mut self, code: IndexIssueCode, message: &str, start: usize, end: usize) {
        self.partial = true;
        self.issues.push(IndexIssue {
            code,
            message: message.to_owned(),
            relative_path: Some(self.path.to_owned()),
            range: Some(self.range(start, end.min(self.bytes.len()))),
        });
    }

    fn range(&self, start: usize, end: usize) -> IndexSourceRange {
        let start = start.min(self.bytes.len());
        let end = end.max(start).min(self.bytes.len());
        let (start_line, start_column) = self.line_column(start);
        let (end_line, end_column) = self.line_column(end);
        IndexSourceRange {
            start_byte: start as u64,
            end_byte: end as u64,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    fn line_column(&self, byte: usize) -> (u32, u32) {
        let line_index = self.line_starts.partition_point(|start| *start <= byte) - 1;
        let line_start = self.line_starts[line_index];
        let column = self.source[line_start..byte].chars().count() + 1;
        ((line_index + 1) as u32, column as u32)
    }
}

struct Group {
    content_start: usize,
    content_end: usize,
    end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(source: &str) -> IndexedFile {
        scan_latex_file(
            "main.tex",
            &"a".repeat(64),
            source,
            ScannerLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_invalid_paths_and_fingerprints_before_scanning() {
        assert_eq!(
            scan_latex_file(
                "../escape.tex",
                &"a".repeat(64),
                "text",
                ScannerLimits::default()
            ),
            Err(ScanError::InvalidRelativePath)
        );
        assert_eq!(
            scan_latex_file("main.tex", "not-a-digest", "text", ScannerLimits::default()),
            Err(ScanError::InvalidFingerprint)
        );
    }

    #[test]
    fn extracts_standard_constructs_packages_macros_and_cryptocode() {
        let result = scan(
            r#"\documentclass{article}
\usepackage[operators]{cryptocode,amsmath}
\newcommand{\KeySpace}{\mathcal{K}}
\section{Security}
\label{sec:security}
See \cref{sec:proof} and \cite{goldreich,bellare}.
\input{sections/proof}
\begin{game}
\pseudocode{Alice \> Bob}
\end{game}
"#,
        );
        assert_eq!(result.status, FileIndexStatus::Complete);
        let pairs: Vec<_> = result
            .records
            .iter()
            .map(|record| (record.kind, record.name.as_str()))
            .collect();
        assert!(pairs.contains(&(IndexRecordKind::Package, "cryptocode")));
        assert!(pairs.contains(&(IndexRecordKind::Package, "amsmath")));
        assert!(pairs.contains(&(IndexRecordKind::MacroDefinition, "KeySpace")));
        assert!(pairs.contains(&(IndexRecordKind::Section, "Security")));
        assert!(pairs.contains(&(IndexRecordKind::Label, "sec:security")));
        assert!(pairs.contains(&(IndexRecordKind::Reference, "sec:proof")));
        assert!(pairs.contains(&(IndexRecordKind::Citation, "goldreich")));
        assert!(pairs.contains(&(IndexRecordKind::Citation, "bellare")));
        assert!(pairs.contains(&(IndexRecordKind::Include, "sections/proof")));
        assert!(pairs.contains(&(IndexRecordKind::Environment, "game")));
        assert!(pairs.contains(&(IndexRecordKind::Cryptocode, "pseudocode")));
        assert!(result.records.iter().all(|record| {
            record.confidence == IndexConfidence::Exact
                && record.provenance == IndexProvenance::Lexical
                && record.range.start_byte <= record.range.end_byte
        }));
    }

    #[test]
    fn ignores_comments_inline_verb_and_verbatim_environments() {
        let result = scan(
            r#"% \section{hidden}
Visible \verb|\label{hidden}| text.
\begin{verbatim}
\cite{hidden}
\end{verbatim}
\label{visible}
"#,
        );
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].kind, IndexRecordKind::Environment);
        assert_eq!(result.records[1].name, "visible");
    }

    #[test]
    fn malformed_input_recovers_and_reports_partial_results() {
        let result = scan("\\section missing\n\\label{kept}\n\\cite{broken");
        assert_eq!(result.status, FileIndexStatus::Partial);
        assert!(result.records.iter().any(|record| record.name == "kept"));
        assert!(
            result
                .issues
                .iter()
                .all(|issue| issue.code == IndexIssueCode::MalformedInput)
        );
    }

    #[test]
    fn enforces_file_record_depth_and_command_limits() {
        let limits = ScannerLimits {
            max_file_bytes: 4,
            ..ScannerLimits::default()
        };
        let result = scan_latex_file("main.tex", &"a".repeat(64), "12345", limits).unwrap();
        assert_eq!(result.status, FileIndexStatus::Skipped);
        assert_eq!(result.issues[0].code, IndexIssueCode::FileTooLarge);

        let limits = ScannerLimits {
            max_records_per_file: 1,
            ..ScannerLimits::default()
        };
        let result = scan_latex_file(
            "main.tex",
            &"a".repeat(64),
            "\\label{one}\\label{two}",
            limits,
        )
        .unwrap();
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.issues[0].code, IndexIssueCode::RecordLimitReached);

        let limits = ScannerLimits {
            max_brace_depth: 2,
            ..ScannerLimits::default()
        };
        let result = scan_latex_file(
            "main.tex",
            &"a".repeat(64),
            "\\section{{{too deep}}}\\label{later}",
            limits,
        )
        .unwrap();
        assert_eq!(result.status, FileIndexStatus::Partial);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == IndexIssueCode::BraceDepthLimitReached)
        );

        let limits = ScannerLimits {
            max_command_bytes: 4,
            ..ScannerLimits::default()
        };
        let result = scan_latex_file("main.tex", &"a".repeat(64), "\\commandtoolong{x}", limits);
        let result = result.unwrap();
        assert_eq!(result.issues[0].code, IndexIssueCode::CommandTooLong);
    }

    #[test]
    fn unicode_ranges_are_one_based_and_scanning_stays_bounded_on_large_lines() {
        let result =
            scan("\u{03b1}\u{03b2} \\label{\u{03ba}\u{03bb}\u{03b5}\u{03b9}\u{03b4}\u{03af}}");
        let record = &result.records[0];
        assert_eq!(record.range.start_line, 1);
        assert_eq!(record.range.start_column, 4);

        let source = format!("{}\\label{{end}}", "x".repeat(100_000));
        let result = scan(&source);
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].name, "end");
    }

    #[test]
    fn arbitrary_malformed_corpus_never_panics_or_emits_invalid_ranges() {
        let unicode_command = scan("\\u{03b1} text \\label{safe}");
        assert!(
            unicode_command
                .records
                .iter()
                .any(|record| record.name == "safe")
        );

        for seed in 0..128_u8 {
            let source = format!(
                "\\section{{x{}}}% comment\n\\verb|{{}}|\\cite{{a,b\\{}",
                "{".repeat((seed % 12) as usize),
                char::from(b'a' + seed % 26)
            );
            let result = scan(&source);
            for record in result.records {
                assert!(record.range.start_byte <= record.range.end_byte);
                assert!(record.range.end_byte <= source.len() as u64);
                assert!(record.range.start_line > 0);
                assert!(record.range.start_column > 0);
            }
        }
    }
}
