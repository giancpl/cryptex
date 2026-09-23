use crate::api::{API_VERSION, OperationId, SynctexPosition};
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

#[derive(Debug, Error)]
pub enum SynctexError {
    #[error("SyncTeX output exceeds the parser limit")]
    OutputTooLarge,
    #[error("SyncTeX returned malformed output")]
    InvalidOutput,
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
