use crate::api::{Diagnostic, DiagnosticPhase, DiagnosticSeverity, DiagnosticSourceRange};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, Read},
    path::{Component, Path},
};

pub const MAX_LATEX_LOG_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedLog {
    pub diagnostics: Vec<Diagnostic>,
    pub truncated: bool,
}

pub fn read_latex_log_file(path: &Path) -> io::Result<(String, bool)> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_LATEX_LOG_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > MAX_LATEX_LOG_BYTES;
    bytes.truncate(MAX_LATEX_LOG_BYTES as usize);
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

pub fn parse_latex_log_file(
    path: &Path,
    project_root: &Path,
    root_document: &str,
) -> io::Result<ParsedLog> {
    let (text, truncated) = read_latex_log_file(path)?;
    let mut parsed = parse_latex_log(&text, project_root, root_document);
    parsed.truncated = truncated;
    Ok(parsed)
}

pub fn parse_latex_log(text: &str, project_root: &Path, root_document: &str) -> ParsedLog {
    let canonical_root = project_root.canonicalize().ok();
    let fallback = normalize_source(root_document, project_root, canonical_root.as_deref());
    let mut files = Vec::<String>::new();
    let mut pending_error: Option<(String, Option<String>)> = None;
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        update_file_stack(line, project_root, canonical_root.as_deref(), &mut files);

        if let Some((path, line_number, message)) =
            parse_file_line_error(line, project_root, canonical_root.as_deref())
        {
            push_diagnostic(
                &mut diagnostics,
                &mut seen,
                "LATEX_ERROR",
                DiagnosticSeverity::Error,
                DiagnosticPhase::Latex,
                message,
                Some(source(path, line_number, line_number)),
            );
            pending_error = None;
            continue;
        }

        if let Some(message) = line.strip_prefix("! ") {
            if !message.starts_with("==>") {
                pending_error = Some((
                    message.trim().to_owned(),
                    files.last().cloned().or_else(|| fallback.clone()),
                ));
            }
            continue;
        }

        if let Some((message, path)) = pending_error.take() {
            if let Some(line_number) = parse_classic_error_line(line) {
                push_diagnostic(
                    &mut diagnostics,
                    &mut seen,
                    "LATEX_ERROR",
                    DiagnosticSeverity::Error,
                    DiagnosticPhase::Latex,
                    message,
                    path.map(|path| source(path, line_number, line_number)),
                );
                continue;
            }
            pending_error = Some((message, path));
        }

        if line.starts_with("Overfull \\hbox") || line.starts_with("Underfull \\hbox") {
            let overfull = line.starts_with("Overfull");
            let range = parse_line_range(line).and_then(|(start, end)| {
                files
                    .last()
                    .cloned()
                    .or_else(|| fallback.clone())
                    .map(|path| source(path, start, end))
            });
            push_diagnostic(
                &mut diagnostics,
                &mut seen,
                if overfull {
                    "OVERFULL_HBOX"
                } else {
                    "UNDERFULL_HBOX"
                },
                DiagnosticSeverity::Warning,
                DiagnosticPhase::Latex,
                line.to_owned(),
                range,
            );
            continue;
        }

        if line.contains("Warning:") {
            let lower = line.to_ascii_lowercase();
            let code = if lower.contains("citation") && lower.contains("undefined") {
                "UNDEFINED_CITATION"
            } else if lower.contains("reference") && lower.contains("undefined") {
                "UNDEFINED_REFERENCE"
            } else {
                "LATEX_WARNING"
            };
            let phase = if lower.starts_with("bibtex")
                || lower.starts_with("biber")
                || lower.contains("biblatex warning")
            {
                DiagnosticPhase::Bibliography
            } else {
                DiagnosticPhase::Latex
            };
            let location = parse_input_line(line).and_then(|line_number| {
                files
                    .last()
                    .cloned()
                    .or_else(|| fallback.clone())
                    .map(|path| source(path, line_number, line_number))
            });
            push_diagnostic(
                &mut diagnostics,
                &mut seen,
                code,
                DiagnosticSeverity::Warning,
                phase,
                line.to_owned(),
                location,
            );
            continue;
        }

        if line.starts_with("Latexmk:")
            && (line.contains("Errors, so I did not complete")
                || line.contains("Failure in processing file"))
        {
            push_diagnostic(
                &mut diagnostics,
                &mut seen,
                "LATEXMK_FAILURE",
                DiagnosticSeverity::Error,
                DiagnosticPhase::Latexmk,
                line.to_owned(),
                None,
            );
        }
    }

    if let Some((message, _)) = pending_error {
        push_diagnostic(
            &mut diagnostics,
            &mut seen,
            "LATEX_ERROR",
            DiagnosticSeverity::Error,
            DiagnosticPhase::Latex,
            message,
            None,
        );
    }

    ParsedLog {
        diagnostics,
        truncated: false,
    }
}

fn update_file_stack(
    line: &str,
    project_root: &Path,
    canonical_root: Option<&Path>,
    files: &mut Vec<String>,
) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'(' {
            index += 1;
            continue;
        }
        let start = index + 1;
        let end = line[start..]
            .find(|character: char| character.is_whitespace() || character == ')')
            .map(|offset| start + offset)
            .unwrap_or(line.len());
        let candidate = line[start..end].trim_matches('"');
        if candidate.ends_with(".tex")
            && let Some(path) = normalize_source(candidate, project_root, canonical_root)
            && files.last() != Some(&path)
        {
            files.push(path);
        }
        index = end.saturating_add(1);
    }
    if line.starts_with(')') {
        for _ in 0..line
            .chars()
            .take_while(|character| *character == ')')
            .count()
        {
            files.pop();
        }
    }
}

fn parse_file_line_error(
    line: &str,
    project_root: &Path,
    canonical_root: Option<&Path>,
) -> Option<(String, u32, String)> {
    let marker = line.find(".tex:")? + 4;
    let path = normalize_source(
        line[..marker].trim_start_matches('(').trim_matches('"'),
        project_root,
        canonical_root,
    )?;
    let rest = line.get(marker + 1..)?;
    let split = rest.find(':')?;
    let line_number = rest[..split].trim().parse().ok()?;
    let message = rest[split + 1..].trim();
    (!message.is_empty()).then(|| (path, line_number, message.to_owned()))
}

fn parse_classic_error_line(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("l.")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn parse_input_line(line: &str) -> Option<u32> {
    let rest = line.split("on input line ").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn parse_line_range(line: &str) -> Option<(u32, u32)> {
    let rest = line
        .rsplit_once(" at lines ")
        .map(|(_, value)| value)
        .or_else(|| line.rsplit_once(" at line ").map(|(_, value)| value))?;
    let mut numbers = rest
        .split(|character: char| !character.is_ascii_digit())
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse::<u32>().ok());
    let start = numbers.next()?;
    let end = numbers.next().unwrap_or(start);
    Some((start, end))
}

fn normalize_source(
    value: &str,
    project_root: &Path,
    canonical_root: Option<&Path>,
) -> Option<String> {
    let raw = Path::new(value);
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        let candidate = Path::new(value.trim_start_matches("./"));
        if candidate
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return None;
        }
        project_root.join(candidate)
    };
    let canonical = joined.canonicalize().ok()?;
    let root = canonical_root?;
    let relative = canonical.strip_prefix(root).ok()?;
    if !canonical.is_file() || relative.as_os_str().is_empty() {
        return None;
    }
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn source(relative_path: String, start_line: u32, end_line: u32) -> DiagnosticSourceRange {
    DiagnosticSourceRange {
        relative_path,
        start_line,
        end_line,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    seen: &mut BTreeSet<(String, Option<String>, Option<u32>, String)>,
    code: &str,
    severity: DiagnosticSeverity,
    phase: DiagnosticPhase,
    message: String,
    source: Option<DiagnosticSourceRange>,
) {
    let key = (
        code.to_owned(),
        source.as_ref().map(|source| source.relative_path.clone()),
        source.as_ref().map(|source| source.start_line),
        message.clone(),
    );
    if seen.insert(key) {
        diagnostics.push(Diagnostic {
            code: code.to_owned(),
            severity,
            phase,
            message,
            source,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parses_file_stack_classic_errors_warnings_and_boxes() {
        let project = tempdir().unwrap();
        fs::create_dir(project.path().join("sections")).unwrap();
        fs::write(project.path().join("main.tex"), "root").unwrap();
        fs::write(project.path().join("sections/intro.tex"), "included").unwrap();
        let log = r#"(./main.tex
(./sections/intro.tex
! Undefined control sequence.
l.42 \bad
)
LaTeX Warning: Reference `sec:x' on page 1 undefined on input line 9.
Overfull \hbox (1.0pt too wide) in paragraph at lines 12--13
)"#;
        let parsed = parse_latex_log(log, project.path(), "main.tex");
        assert_eq!(parsed.diagnostics.len(), 3);
        assert_eq!(parsed.diagnostics[0].code, "LATEX_ERROR");
        assert_eq!(
            parsed.diagnostics[0].source.as_ref().unwrap().relative_path,
            "sections/intro.tex"
        );
        assert_eq!(
            parsed.diagnostics[0].source.as_ref().unwrap().start_line,
            42
        );
        assert_eq!(parsed.diagnostics[1].code, "UNDEFINED_REFERENCE");
        assert_eq!(
            parsed.diagnostics[1].source.as_ref().unwrap().relative_path,
            "main.tex"
        );
        assert_eq!(parsed.diagnostics[2].source.as_ref().unwrap().end_line, 13);
    }

    #[test]
    fn parses_file_line_errors_and_bibliography_warnings() {
        let project = tempdir().unwrap();
        fs::write(project.path().join("main.tex"), "root").unwrap();
        let log = "./main.tex:7: Missing $ inserted.\nPackage biblatex Warning: Citation `key' undefined on input line 5.\nLatexmk: Errors, so I did not complete making targets\n";
        let parsed = parse_latex_log(log, project.path(), "main.tex");
        assert_eq!(parsed.diagnostics.len(), 3);
        assert_eq!(parsed.diagnostics[0].source.as_ref().unwrap().start_line, 7);
        assert_eq!(parsed.diagnostics[1].phase, DiagnosticPhase::Bibliography);
        assert_eq!(parsed.diagnostics[2].code, "LATEXMK_FAILURE");
        assert!(parsed.diagnostics[2].source.is_none());
    }

    #[test]
    fn accepts_absolute_sources_only_when_they_resolve_inside_the_project() {
        let project = tempdir().unwrap();
        let source = project.path().join("main.tex");
        fs::write(&source, "root").unwrap();
        let parsed = parse_latex_log(
            &format!("{}:11: Undefined control sequence.", source.display()),
            project.path(),
            "main.tex",
        );
        assert_eq!(
            parsed.diagnostics[0].source.as_ref().unwrap().relative_path,
            "main.tex"
        );
    }

    #[test]
    fn malformed_and_outside_paths_never_gain_false_locations() {
        let project = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(project.path().join("main.tex"), "root").unwrap();
        fs::write(outside.path().join("outside.tex"), "outside").unwrap();
        let log = format!(
            "{}:3: hostile\n! Truncated error",
            outside.path().join("outside.tex").display()
        );
        let parsed = parse_latex_log(&log, project.path(), "missing.tex");
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].message, "Truncated error");
        assert!(parsed.diagnostics[0].source.is_none());
    }

    #[test]
    fn file_reader_bounds_untrusted_logs() {
        let project = tempdir().unwrap();
        fs::write(project.path().join("main.tex"), "root").unwrap();
        let log = project.path().join("main.log");
        fs::write(&log, vec![b'x'; MAX_LATEX_LOG_BYTES as usize + 32]).unwrap();
        let parsed = parse_latex_log_file(&log, project.path(), "main.tex").unwrap();
        assert!(parsed.truncated);
        assert!(parsed.diagnostics.is_empty());
    }
}
