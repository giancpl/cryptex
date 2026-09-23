//! Reviewed exact-form notation replacements.
use crate::{
    api::API_VERSION, index::IndexSourceRange, notation::EffectiveNotationProfile,
    notation_usage::ProjectNotationUsage,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub const NOTATION_REFACTOR_SCHEMA_VERSION: u16 = 1;
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationRenameEdit {
    pub range: IndexSourceRange,
    pub from: String,
    pub to: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationRenameFilePreview {
    pub relative_path: String,
    pub fingerprint: String,
    pub original_text: String,
    pub revised_text: String,
    pub edits: Vec<NotationRenameEdit>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationRenamePreview {
    pub api_version: u16,
    pub schema_version: u16,
    pub project_id: String,
    pub concept_id: String,
    pub preferred_form: String,
    #[ts(type = "bigint")]
    pub index_generation: u64,
    pub files: Vec<NotationRenameFilePreview>,
    pub incomplete: bool,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationRenameFileSelection {
    pub relative_path: String,
    pub expected_fingerprint: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ApplyNotationRenameRequest {
    pub project_id: String,
    pub concept_id: String,
    pub files: Vec<NotationRenameFileSelection>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum NotationRenameFileStatus {
    Applied,
    Changed,
    Failed,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationRenameFileResult {
    pub relative_path: String,
    pub status: NotationRenameFileStatus,
    pub message: Option<String>,
    pub fingerprint: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ApplyNotationRenameResult {
    pub api_version: u16,
    pub project_id: String,
    pub concept_id: String,
    pub files: Vec<NotationRenameFileResult>,
}
pub fn preview_notation_rename<F>(
    usage: &ProjectNotationUsage,
    profile: &EffectiveNotationProfile,
    concept_id: &str,
    mut read: F,
) -> Result<NotationRenamePreview, String>
where
    F: FnMut(&str) -> Option<(String, String)>,
{
    let concept = profile
        .concepts
        .iter()
        .find(|x| x.id == concept_id)
        .ok_or_else(|| format!("unknown notation concept: {concept_id}"))?;
    let (mut files, mut incomplete) = (Vec::new(), usage.incomplete);
    for file in &usage.files {
        let mut found = file
            .usages
            .iter()
            .filter(|x| x.concept_id == concept_id && !x.preferred)
            .collect::<Vec<_>>();
        if found.is_empty() {
            continue;
        }
        let Some((source, fingerprint)) = read(&file.relative_path) else {
            incomplete = true;
            continue;
        };
        if fingerprint != file.fingerprint {
            incomplete = true;
            continue;
        }
        found.sort_by_key(|x| x.range.start_byte);
        let (mut revised, mut edits) = (source.clone(), Vec::new());
        for item in found.into_iter().rev() {
            let (a, b) = (item.range.start_byte as usize, item.range.end_byte as usize);
            if source.get(a..b) != Some(item.form.as_str()) {
                incomplete = true;
                continue;
            }
            revised.replace_range(a..b, &concept.preferred_form);
            edits.push(NotationRenameEdit {
                range: item.range,
                from: item.form.clone(),
                to: concept.preferred_form.clone(),
            })
        }
        edits.reverse();
        if !edits.is_empty() {
            files.push(NotationRenameFilePreview {
                relative_path: file.relative_path.clone(),
                fingerprint,
                original_text: source,
                revised_text: revised,
                edits,
            })
        }
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(NotationRenamePreview {
        api_version: API_VERSION,
        schema_version: NOTATION_REFACTOR_SCHEMA_VERSION,
        project_id: usage.project_id.clone(),
        concept_id: concept_id.into(),
        preferred_form: concept.preferred_form.clone(),
        index_generation: usage.index_generation,
        files,
        incomplete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        notation::{EffectiveNotationConcept, NotationPreferenceSource},
        notation_usage::{NotationFileUsage, NotationUsage},
    };

    fn range(start: u64, end: u64) -> IndexSourceRange {
        IndexSourceRange {
            start_byte: start,
            end_byte: end,
            start_line: 1,
            start_column: (start + 1) as u32,
            end_line: 1,
            end_column: (end + 1) as u32,
        }
    }

    fn profile() -> EffectiveNotationProfile {
        EffectiveNotationProfile {
            api_version: API_VERSION,
            profile_version: 1,
            name: "Test".into(),
            project_id: Some("p".into()),
            concepts: vec![EffectiveNotationConcept {
                id: "probability".into(),
                label: "Probability".into(),
                preferred_form: "\\Pr".into(),
                declared_forms: vec!["\\Pr".into(), "\\P".into()],
                source: NotationPreferenceSource::Global,
            }],
        }
    }

    fn usage() -> ProjectNotationUsage {
        ProjectNotationUsage {
            api_version: API_VERSION,
            schema_version: 1,
            profile_version: 1,
            project_id: "p".into(),
            index_generation: 7,
            incomplete: false,
            files: vec![NotationFileUsage {
                relative_path: "main.tex".into(),
                fingerprint: "a".repeat(64),
                truncated: false,
                usages: vec![
                    NotationUsage {
                        concept_id: "probability".into(),
                        form: "\\P".into(),
                        preferred: false,
                        range: range(1, 3),
                    },
                    NotationUsage {
                        concept_id: "probability".into(),
                        form: "\\Pr".into(),
                        preferred: true,
                        range: range(6, 9),
                    },
                    NotationUsage {
                        concept_id: "probability".into(),
                        form: "\\P".into(),
                        preferred: false,
                        range: range(12, 14),
                    },
                ],
            }],
        }
    }

    #[test]
    fn previews_exact_nonpreferred_forms_from_end_to_start() {
        let source = "$\\P + \\Pr + \\P$".to_owned();
        let preview = preview_notation_rename(&usage(), &profile(), "probability", |_| {
            Some((source.clone(), "a".repeat(64)))
        })
        .unwrap();
        assert_eq!(preview.files[0].revised_text, "$\\Pr + \\Pr + \\Pr$");
        assert_eq!(preview.files[0].edits.len(), 2);
    }

    #[test]
    fn excludes_changed_files_instead_of_rewriting_them() {
        let preview = preview_notation_rename(&usage(), &profile(), "probability", |_| {
            Some(("$\\P + \\Pr + \\P$".into(), "b".repeat(64)))
        })
        .unwrap();
        assert!(preview.files.is_empty());
        assert!(preview.incomplete);
    }

    #[test]
    fn rejects_unknown_concepts() {
        assert!(preview_notation_rename(&usage(), &profile(), "missing", |_| None).is_err());
    }
}
