use crate::api::{API_VERSION, OperationId, SynctexPosition};
use std::path::{Path, PathBuf};
use thiserror::Error;

const MAX_SYNCTEX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_PAGE: u32 = 1_000_000;
const MAX_COORDINATE: f64 = 1_000_000.0;

pub fn parse_forward_output(
    output: &[u8],
    project_id: &str,
    operation_id: OperationId,
) -> Result<Option<SynctexPosition>, SynctexError> {
    if output.len() > MAX_SYNCTEX_OUTPUT_BYTES {
        return Err(SynctexError::OutputTooLarge);
    }
    let text = std::str::from_utf8(output).map_err(|_| SynctexError::InvalidOutput)?;
    let body = text
        .split_once("SyncTeX result begin")
        .and_then(|(_, rest)| rest.split_once("SyncTeX result end").map(|(body, _)| body))
        .ok_or(SynctexError::InvalidOutput)?;
    let mut page = None;
    let mut horizontal = None;
    let mut vertical = None;
    let mut width = None;
    let mut height = None;
    for line in body.lines().map(str::trim) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key {
            "Page" if page.is_none() => {
                page = value.parse::<u32>().ok().filter(|page| *page <= MAX_PAGE)
            }
            "h" if horizontal.is_none() => horizontal = bounded(value),
            "v" if vertical.is_none() => vertical = bounded(value),
            "W" if width.is_none() => width = bounded(value),
            "H" if height.is_none() => height = bounded(value),
            _ => {}
        }
        if page.is_some()
            && horizontal.is_some()
            && vertical.is_some()
            && width.is_some()
            && height.is_some()
        {
            break;
        }
    }
    let Some((page, x, y)) = page
        .filter(|page| *page > 0)
        .zip(horizontal)
        .zip(vertical)
        .map(|((page, x), y)| (page, x, y))
    else {
        return Ok(None);
    };
    Ok(Some(SynctexPosition {
        api_version: API_VERSION,
        project_id: project_id.to_owned(),
        operation_id,
        page,
        x: x.max(0.0),
        y: y.max(0.0),
        width: width.unwrap_or(0.0).max(0.0),
        height: height.unwrap_or(0.0).max(0.0),
    }))
}

fn bounded(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && value.abs() <= MAX_COORDINATE)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynctexEditResult {
    pub input: String,
    pub line: u32,
    pub column: Option<u32>,
}

pub fn parse_inverse_output(output: &[u8]) -> Result<Option<SynctexEditResult>, SynctexError> {
    if output.len() > MAX_SYNCTEX_OUTPUT_BYTES {
        return Err(SynctexError::OutputTooLarge);
    }
    let text = std::str::from_utf8(output).map_err(|_| SynctexError::InvalidOutput)?;
    let body = text
        .split_once("SyncTeX result begin")
        .and_then(|(_, rest)| rest.split_once("SyncTeX result end").map(|(body, _)| body))
        .ok_or(SynctexError::InvalidOutput)?;
    let mut input = None;
    let mut line = None;
    let mut column = None;
    for entry in body.lines().map(str::trim) {
        let Some((key, value)) = entry.split_once(':') else {
            continue;
        };
        match key {
            "Input" if input.is_none() && !value.is_empty() => input = Some(value.to_owned()),
            "Line" if line.is_none() => {
                line = value
                    .parse::<u32>()
                    .ok()
                    .filter(|line| *line > 0 && *line <= i32::MAX as u32)
            }
            "Column" if column.is_none() => {
                column = value
                    .parse::<i64>()
                    .ok()
                    .filter(|column| *column > 0 && *column <= i32::MAX as i64)
                    .map(|column| column as u32)
            }
            _ => {}
        }
    }
    Ok(input.zip(line).map(|(input, line)| SynctexEditResult {
        input,
        line,
        column,
    }))
}

pub fn resolve_inverse_source(
    project_root: &Path,
    input: &str,
) -> Result<(PathBuf, String), SynctexError> {
    let root = project_root
        .canonicalize()
        .map_err(|_| SynctexError::UnsafeSource)?;
    let candidate = PathBuf::from(input);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    let resolved = candidate
        .canonicalize()
        .map_err(|_| SynctexError::UnsafeSource)?;
    if !resolved.is_file() || !resolved.starts_with(&root) {
        return Err(SynctexError::UnsafeSource);
    }
    let relative = resolved
        .strip_prefix(&root)
        .ok()
        .and_then(Path::to_str)
        .filter(|path| !path.is_empty())
        .ok_or(SynctexError::UnsafeSource)?
        .to_owned();
    Ok((resolved, relative))
}

#[derive(Debug, Error)]
pub enum SynctexError {
    #[error("SyncTeX output exceeds the parser limit")]
    OutputTooLarge,
    #[error("SyncTeX returned malformed output")]
    InvalidOutput,
    #[error("SyncTeX returned a source outside the open project")]
    UnsafeSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_first_forward_result_into_page_coordinates() {
        let output = br#"This is SyncTeX command line utility, version 1.5
SyncTeX result begin
Output:/cache/build.pdf
Page:8
x:155.0
y:112.0
h:89.25
v:117.5
W:416.75
H:16.5
before:
offset:0
middle:
after:
SyncTeX result end
"#;
        let result = parse_forward_output(
            output,
            &"a".repeat(64),
            OperationId("build-00000000000000000001".to_owned()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.page, 8);
        assert_eq!((result.x, result.y), (89.25, 117.5));
        assert_eq!((result.width, result.height), (416.75, 16.5));
    }

    #[test]
    fn parses_inverse_results_and_treats_negative_columns_as_unknown() {
        let output = br#"This is SyncTeX command line utility, version 1.5
SyncTeX result begin
Output:/cache/build.pdf
Input:/paper/sections/proof.tex
Line:1575
Column:-1
Offset:0
Context:
SyncTeX result end
"#;
        assert_eq!(
            parse_inverse_output(output).unwrap(),
            Some(SynctexEditResult {
                input: "/paper/sections/proof.tex".to_owned(),
                line: 1575,
                column: None,
            })
        );
        assert!(
            parse_inverse_output(b"SyncTeX result begin\nSyncTeX result end")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            parse_inverse_output(
                b"SyncTeX result begin\nInput:chapter.tex\nLine:12\nColumn:7\nSyncTeX result end",
            )
            .unwrap()
            .unwrap()
            .column,
            Some(7)
        );
    }

    #[test]
    fn inverse_sources_must_resolve_to_regular_files_inside_the_project() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("sections")).unwrap();
        std::fs::write(project.path().join("sections/proof.tex"), "proof").unwrap();
        std::fs::write(outside.path().join("secret.tex"), "secret").unwrap();

        let (resolved, relative) =
            resolve_inverse_source(project.path(), "sections/proof.tex").unwrap();
        assert_eq!(relative, "sections/proof.tex");
        assert_eq!(resolved, project.path().join("sections/proof.tex"));
        assert!(matches!(
            resolve_inverse_source(
                project.path(),
                outside.path().join("secret.tex").to_str().unwrap()
            ),
            Err(SynctexError::UnsafeSource)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn inverse_sources_reject_symlinks_that_escape_the_project() {
        use std::os::unix::fs::symlink;
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.tex"), "secret").unwrap();
        symlink(
            outside.path().join("secret.tex"),
            project.path().join("escape.tex"),
        )
        .unwrap();
        assert!(matches!(
            resolve_inverse_source(project.path(), "escape.tex"),
            Err(SynctexError::UnsafeSource)
        ));
    }

    #[test]
    fn degrades_missing_results_and_rejects_malformed_or_nonfinite_output() {
        assert!(
            parse_forward_output(
                b"SyncTeX result begin\nSyncTeX result end",
                "project",
                OperationId("operation".to_owned()),
            )
            .unwrap()
            .is_none()
        );
        assert!(matches!(
            parse_forward_output(
                b"not a result",
                "project",
                OperationId("operation".to_owned()),
            ),
            Err(SynctexError::InvalidOutput)
        ));
        assert!(
            parse_forward_output(
                b"SyncTeX result begin\nPage:1\nh:NaN\nv:2\nSyncTeX result end",
                "project",
                OperationId("operation".to_owned()),
            )
            .unwrap()
            .is_none()
        );
    }
}
