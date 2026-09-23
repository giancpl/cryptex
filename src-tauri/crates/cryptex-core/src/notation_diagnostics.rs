//! Deterministic notation consistency diagnostics from exact I3 evidence.

use crate::{
    api::API_VERSION,
    index::IndexSourceRange,
    notation::{EffectiveNotationProfile, NotationSuppression},
    notation_usage::{NotationUsage, ProjectNotationUsage},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use ts_rs::TS;

pub const NOTATION_DIAGNOSTICS_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum NotationDiagnosticSeverity {
    Information,
    Warning,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum NotationDiagnosticCode {
    NonPreferredDeclaredForm,
    MultipleDeclaredForms,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationDiagnostic {
    pub code: NotationDiagnosticCode,
    pub severity: NotationDiagnosticSeverity,
    pub concept_id: String,
    pub concept_label: String,
    pub message: String,
    pub relative_path: String,
    pub fingerprint: String,
    pub range: IndexSourceRange,
    pub observed_form: String,
    pub preferred_form: String,
    pub evidence_forms: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectNotationDiagnostics {
    pub api_version: u16,
    pub schema_version: u16,
    pub profile_version: u16,
    pub project_id: String,
    pub index_generation: u64,
    pub diagnostics: Vec<NotationDiagnostic>,
    pub suppressed_count: u32,
    pub incomplete: bool,
}

struct LocatedUsage<'a> {
    relative_path: &'a str,
    fingerprint: &'a str,
    usage: &'a NotationUsage,
}

pub fn analyze_notation_consistency(
    usage: &ProjectNotationUsage,
    profile: &EffectiveNotationProfile,
    suppressions: &[NotationSuppression],
) -> ProjectNotationDiagnostics {
    let concepts: HashMap<_, _> = profile
        .concepts
        .iter()
        .map(|concept| (concept.id.as_str(), concept))
        .collect();
    let mut grouped: BTreeMap<&str, Vec<LocatedUsage<'_>>> = BTreeMap::new();
    for file in &usage.files {
        for item in &file.usages {
            grouped
                .entry(item.concept_id.as_str())
                .or_default()
                .push(LocatedUsage {
                    relative_path: &file.relative_path,
                    fingerprint: &file.fingerprint,
                    usage: item,
                });
        }
    }

    let mut diagnostics = Vec::new();
    let mut suppressed_count = 0_u32;
    for (concept_id, occurrences) in grouped {
        let Some(concept) = concepts.get(concept_id) else {
            continue;
        };
        let evidence_forms: Vec<String> = occurrences
            .iter()
            .map(|located| located.usage.form.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let inconsistent = evidence_forms.len() > 1;
        for located in occurrences {
            if located.usage.preferred {
                continue;
            }
            if is_suppressed(suppressions, concept_id, located.relative_path) {
                suppressed_count = suppressed_count.saturating_add(1);
                continue;
            }
            let (code, severity, message) = if inconsistent {
                (
                    NotationDiagnosticCode::MultipleDeclaredForms,
                    NotationDiagnosticSeverity::Warning,
                    format!(
                        "Multiple declared forms for {} were observed ({}); this exact occurrence uses `{}` while the preferred form is `{}`.",
                        concept.label,
                        evidence_forms.join(", "),
                        located.usage.form,
                        concept.preferred_form
                    ),
                )
            } else {
                (
                    NotationDiagnosticCode::NonPreferredDeclaredForm,
                    NotationDiagnosticSeverity::Information,
                    format!(
                        "This exact declared form for {} is `{}`; the configured preferred form is `{}`.",
                        concept.label, located.usage.form, concept.preferred_form
                    ),
                )
            };
            diagnostics.push(NotationDiagnostic {
                code,
                severity,
                concept_id: concept_id.to_owned(),
                concept_label: concept.label.clone(),
                message,
                relative_path: located.relative_path.to_owned(),
                fingerprint: located.fingerprint.to_owned(),
                range: located.usage.range,
                observed_form: located.usage.form.clone(),
                preferred_form: concept.preferred_form.clone(),
                evidence_forms: evidence_forms.clone(),
            });
        }
    }
    diagnostics.sort_by(|left, right| {
        (&left.relative_path, left.range.start_byte, &left.concept_id).cmp(&(
            &right.relative_path,
            right.range.start_byte,
            &right.concept_id,
        ))
    });
    ProjectNotationDiagnostics {
        api_version: API_VERSION,
        schema_version: NOTATION_DIAGNOSTICS_SCHEMA_VERSION,
        profile_version: profile.profile_version,
        project_id: usage.project_id.clone(),
        index_generation: usage.index_generation,
        diagnostics,
        suppressed_count,
        incomplete: usage.incomplete,
    }
}

fn is_suppressed(
    suppressions: &[NotationSuppression],
    concept_id: &str,
    relative_path: &str,
) -> bool {
    suppressions.iter().any(|suppression| {
        suppression.concept_id == concept_id
            && suppression
                .relative_path
                .as_deref()
                .is_none_or(|path| path == relative_path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        notation::{EffectiveNotationConcept, NotationPreferenceSource},
        notation_usage::{NotationFileUsage, NotationUsage},
    };

    fn range(byte: u64) -> IndexSourceRange {
        IndexSourceRange {
            start_byte: byte,
            end_byte: byte + 3,
            start_line: 1,
            start_column: byte as u32 + 1,
            end_line: 1,
            end_column: byte as u32 + 4,
        }
    }

    fn profile() -> EffectiveNotationProfile {
        EffectiveNotationProfile {
            api_version: 1,
            profile_version: 1,
            name: "test".to_owned(),
            project_id: Some("a".repeat(64)),
            concepts: vec![EffectiveNotationConcept {
                id: "adversary".to_owned(),
                label: "Adversary".to_owned(),
                preferred_form: "\\mathcal{A}".to_owned(),
                declared_forms: vec!["\\mathcal{A}".to_owned(), "\\mathsf{A}".to_owned()],
                source: NotationPreferenceSource::Default,
            }],
        }
    }

    fn usage(forms: &[(&str, bool)]) -> ProjectNotationUsage {
        ProjectNotationUsage {
            api_version: 1,
            schema_version: 1,
            profile_version: 1,
            project_id: "a".repeat(64),
            index_generation: 4,
            files: vec![NotationFileUsage {
                relative_path: "main.tex".to_owned(),
                fingerprint: "b".repeat(64),
                usages: forms
                    .iter()
                    .enumerate()
                    .map(|(index, (form, preferred))| NotationUsage {
                        concept_id: "adversary".to_owned(),
                        form: (*form).to_owned(),
                        preferred: *preferred,
                        range: range(index as u64 * 10),
                    })
                    .collect(),
                truncated: false,
            }],
            incomplete: false,
        }
    }

    #[test]
    fn preferred_only_usage_has_no_diagnostic() {
        let result =
            analyze_notation_consistency(&usage(&[("\\mathcal{A}", true)]), &profile(), &[]);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn exact_nonpreferred_only_usage_is_informational() {
        let result =
            analyze_notation_consistency(&usage(&[("\\mathsf{A}", false)]), &profile(), &[]);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].code,
            NotationDiagnosticCode::NonPreferredDeclaredForm
        );
        assert_eq!(
            result.diagnostics[0].severity,
            NotationDiagnosticSeverity::Information
        );
        assert!(
            result.diagnostics[0]
                .message
                .contains("exact declared form")
        );
    }

    #[test]
    fn multiple_exact_forms_raise_warning_only_on_nonpreferred_occurrences() {
        let result = analyze_notation_consistency(
            &usage(&[("\\mathcal{A}", true), ("\\mathsf{A}", false)]),
            &profile(),
            &[],
        );
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].code,
            NotationDiagnosticCode::MultipleDeclaredForms
        );
        assert_eq!(result.diagnostics[0].evidence_forms.len(), 2);
        assert!(!result.diagnostics[0].message.contains("wrong"));
    }

    #[test]
    fn project_and_file_suppressions_are_deterministic() {
        let evidence = usage(&[("\\mathsf{A}", false)]);
        for relative_path in [None, Some("main.tex".to_owned())] {
            let result = analyze_notation_consistency(
                &evidence,
                &profile(),
                &[NotationSuppression {
                    concept_id: "adversary".to_owned(),
                    relative_path,
                }],
            );
            assert!(result.diagnostics.is_empty());
            assert_eq!(result.suppressed_count, 1);
        }
    }
}
