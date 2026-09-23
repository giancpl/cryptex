//! Versioned, external notation profiles.
//!
//! Profiles describe literal LaTeX forms only. They do not assert mathematical
//! equivalence and they never write into a project directory.

use crate::{api::API_VERSION, project::ProjectId};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;
use ts_rs::TS;

pub const NOTATION_PROFILE_VERSION: u16 = 1;
const MAX_PROFILE_BYTES: usize = 256 * 1024;
const MAX_CONCEPTS: usize = 512;
const MAX_FORMS_PER_CONCEPT: usize = 64;
const MAX_TEXT_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationConcept {
    pub id: String,
    pub label: String,
    pub preferred_form: String,
    pub declared_forms: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationProfile {
    pub version: u16,
    pub name: String,
    pub concepts: Vec<NotationConcept>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct NotationConceptOverride {
    pub concept_id: String,
    pub preferred_form: String,
    pub declared_forms: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectNotationOverrides {
    pub version: u16,
    pub concepts: Vec<NotationConceptOverride>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum NotationPreferenceSource {
    Default,
    Global,
    Project,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct EffectiveNotationConcept {
    pub id: String,
    pub label: String,
    pub preferred_form: String,
    pub declared_forms: Vec<String>,
    pub source: NotationPreferenceSource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct EffectiveNotationProfile {
    pub api_version: u16,
    pub profile_version: u16,
    pub name: String,
    pub project_id: Option<String>,
    pub concepts: Vec<EffectiveNotationConcept>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredNotationProfiles {
    version: u16,
    global: Option<NotationProfile>,
    projects: HashMap<String, ProjectNotationOverrides>,
}

pub struct NotationService {
    path: PathBuf,
    global: Option<NotationProfile>,
    projects: HashMap<String, ProjectNotationOverrides>,
}

impl NotationService {
    pub fn load(path: PathBuf) -> Result<Self, NotationError> {
        let stored = match fs::read(&path) {
            Ok(bytes) => {
                if bytes.len() > MAX_PROFILE_BYTES {
                    return Err(NotationError::TooLarge);
                }
                let stored: StoredNotationProfiles =
                    serde_json::from_slice(&bytes).map_err(NotationError::InvalidJson)?;
                if stored.version != NOTATION_PROFILE_VERSION {
                    return Err(NotationError::UnsupportedVersion(stored.version));
                }
                stored
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoredNotationProfiles {
                version: NOTATION_PROFILE_VERSION,
                global: None,
                projects: HashMap::new(),
            },
            Err(error) => return Err(NotationError::Io(error)),
        };
        if let Some(profile) = &stored.global {
            validate_profile(profile)?;
        }
        for (project_id, overrides) in &stored.projects {
            ProjectId::parse(project_id).map_err(|_| NotationError::InvalidProject)?;
            validate_overrides(
                overrides,
                stored.global.as_ref().unwrap_or(&default_profile()),
            )?;
        }
        Ok(Self {
            path,
            global: stored.global,
            projects: stored.projects,
        })
    }

    pub fn effective(
        &self,
        project_id: Option<&str>,
    ) -> Result<EffectiveNotationProfile, NotationError> {
        if let Some(project_id) = project_id {
            ProjectId::parse(project_id).map_err(|_| NotationError::InvalidProject)?;
        }
        let base = self.global.clone().unwrap_or_else(default_profile);
        let base_source = if self.global.is_some() {
            NotationPreferenceSource::Global
        } else {
            NotationPreferenceSource::Default
        };
        let overrides = project_id.and_then(|id| self.projects.get(id));
        let by_id: HashMap<&str, &NotationConceptOverride> = overrides
            .map(|value| {
                value
                    .concepts
                    .iter()
                    .map(|item| (item.concept_id.as_str(), item))
                    .collect()
            })
            .unwrap_or_default();
        Ok(EffectiveNotationProfile {
            api_version: API_VERSION,
            profile_version: NOTATION_PROFILE_VERSION,
            name: base.name,
            project_id: project_id.map(str::to_owned),
            concepts: base
                .concepts
                .into_iter()
                .map(|concept| match by_id.get(concept.id.as_str()) {
                    Some(value) => EffectiveNotationConcept {
                        id: concept.id,
                        label: concept.label,
                        preferred_form: value.preferred_form.clone(),
                        declared_forms: value.declared_forms.clone(),
                        source: NotationPreferenceSource::Project,
                    },
                    None => EffectiveNotationConcept {
                        id: concept.id,
                        label: concept.label,
                        preferred_form: concept.preferred_form,
                        declared_forms: concept.declared_forms,
                        source: base_source,
                    },
                })
                .collect(),
        })
    }

    pub fn set_global(&mut self, profile: NotationProfile) -> Result<(), NotationError> {
        validate_profile(&profile)?;
        for overrides in self.projects.values() {
            validate_overrides(overrides, &profile)?;
        }
        let previous = self.global.replace(profile);
        if let Err(error) = self.persist() {
            self.global = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn reset_global(&mut self) -> Result<(), NotationError> {
        let defaults = default_profile();
        for overrides in self.projects.values() {
            validate_overrides(overrides, &defaults)?;
        }
        let previous = self.global.take();
        if let Err(error) = self.persist() {
            self.global = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_project_overrides(
        &mut self,
        project_id: &str,
        overrides: ProjectNotationOverrides,
    ) -> Result<(), NotationError> {
        let project_id = ProjectId::parse(project_id).map_err(|_| NotationError::InvalidProject)?;
        let base = self
            .global
            .as_ref()
            .cloned()
            .unwrap_or_else(default_profile);
        validate_overrides(&overrides, &base)?;
        let key = project_id.as_str().to_owned();
        let previous = if overrides.concepts.is_empty() {
            self.projects.remove(&key)
        } else {
            self.projects.insert(key.clone(), overrides)
        };
        if let Err(error) = self.persist() {
            if let Some(previous) = previous {
                self.projects.insert(key, previous);
            } else {
                self.projects.remove(&key);
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn import_global(&mut self, json: &str) -> Result<NotationProfile, NotationError> {
        if json.len() > MAX_PROFILE_BYTES {
            return Err(NotationError::TooLarge);
        }
        let profile: NotationProfile =
            serde_json::from_str(json).map_err(NotationError::InvalidJson)?;
        self.set_global(profile.clone())?;
        Ok(profile)
    }

    pub fn export_global(&self) -> Result<String, NotationError> {
        serde_json::to_string_pretty(self.global.as_ref().unwrap_or(&default_profile()))
            .map_err(NotationError::Serialize)
    }

    fn persist(&self) -> Result<(), NotationError> {
        let parent = self.path.parent().ok_or(NotationError::InvalidLocation)?;
        fs::create_dir_all(parent).map_err(NotationError::Io)?;
        restrict_directory(parent)?;
        let bytes = serde_json::to_vec_pretty(&StoredNotationProfiles {
            version: NOTATION_PROFILE_VERSION,
            global: self.global.clone(),
            projects: self.projects.clone(),
        })
        .map_err(NotationError::Serialize)?;
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(NotationError::TooLarge);
        }
        let mut temporary = NamedTempFile::new_in(parent).map_err(NotationError::Io)?;
        temporary.write_all(&bytes).map_err(NotationError::Io)?;
        temporary.flush().map_err(NotationError::Io)?;
        temporary.as_file().sync_all().map_err(NotationError::Io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| NotationError::Io(error.error))?;
        sync_directory(parent)?;
        Ok(())
    }
}

pub fn default_profile() -> NotationProfile {
    let concepts = [
        (
            "security-parameter",
            "Security parameter",
            "\\lambda",
            ["\\lambda"].as_slice(),
        ),
        (
            "adversary",
            "Adversary",
            "\\mathcal{A}",
            ["\\mathcal{A}", "\\mathsf{A}"].as_slice(),
        ),
        (
            "challenger",
            "Challenger",
            "\\mathcal{C}",
            ["\\mathcal{C}", "\\mathsf{C}"].as_slice(),
        ),
        (
            "negligible-function",
            "Negligible function",
            "\\mathsf{negl}",
            ["\\mathsf{negl}", "\\operatorname{negl}"].as_slice(),
        ),
        (
            "probability",
            "Probability",
            "\\Pr",
            ["\\Pr", "\\mathbb{P}"].as_slice(),
        ),
    ]
    .into_iter()
    .map(|(id, label, preferred, forms)| NotationConcept {
        id: id.to_owned(),
        label: label.to_owned(),
        preferred_form: preferred.to_owned(),
        declared_forms: forms.iter().map(|value| (*value).to_owned()).collect(),
    })
    .collect();
    NotationProfile {
        version: NOTATION_PROFILE_VERSION,
        name: "CrypTex cryptography defaults".to_owned(),
        concepts,
    }
}

pub fn validate_profile(profile: &NotationProfile) -> Result<(), NotationError> {
    if profile.version != NOTATION_PROFILE_VERSION {
        return Err(NotationError::UnsupportedVersion(profile.version));
    }
    validate_text("profile name", &profile.name)?;
    if profile.concepts.is_empty() || profile.concepts.len() > MAX_CONCEPTS {
        return Err(NotationError::InvalidProfile(
            "concept count is out of bounds".to_owned(),
        ));
    }
    let mut ids = HashSet::new();
    let mut all_forms = HashSet::new();
    for concept in &profile.concepts {
        validate_concept_id(&concept.id)?;
        validate_text("concept label", &concept.label)?;
        if !ids.insert(concept.id.as_str()) {
            return Err(NotationError::InvalidProfile(format!(
                "duplicate concept id `{}`",
                concept.id
            )));
        }
        validate_forms(&concept.preferred_form, &concept.declared_forms)?;
        for form in &concept.declared_forms {
            if !all_forms.insert(form.as_str()) {
                return Err(NotationError::InvalidProfile(format!(
                    "form `{form}` is declared by more than one concept"
                )));
            }
        }
    }
    Ok(())
}

fn validate_overrides(
    overrides: &ProjectNotationOverrides,
    profile: &NotationProfile,
) -> Result<(), NotationError> {
    if overrides.version != NOTATION_PROFILE_VERSION {
        return Err(NotationError::UnsupportedVersion(overrides.version));
    }
    if overrides.concepts.len() > profile.concepts.len() {
        return Err(NotationError::InvalidProfile(
            "too many project overrides".to_owned(),
        ));
    }
    let known: HashSet<&str> = profile
        .concepts
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    let mut seen = HashSet::new();
    for item in &overrides.concepts {
        if !known.contains(item.concept_id.as_str()) {
            return Err(NotationError::UnknownConcept(item.concept_id.clone()));
        }
        if !seen.insert(item.concept_id.as_str()) {
            return Err(NotationError::InvalidProfile(format!(
                "duplicate override `{}`",
                item.concept_id
            )));
        }
        validate_forms(&item.preferred_form, &item.declared_forms)?;
    }
    let mut effective = profile.clone();
    for item in &overrides.concepts {
        let concept = effective
            .concepts
            .iter_mut()
            .find(|concept| concept.id == item.concept_id)
            .expect("known override concept");
        concept.preferred_form.clone_from(&item.preferred_form);
        concept.declared_forms.clone_from(&item.declared_forms);
    }
    validate_profile(&effective)
}

fn validate_forms(preferred: &str, forms: &[String]) -> Result<(), NotationError> {
    validate_text("preferred form", preferred)?;
    if forms.is_empty() || forms.len() > MAX_FORMS_PER_CONCEPT {
        return Err(NotationError::InvalidProfile(
            "declared form count is out of bounds".to_owned(),
        ));
    }
    if !forms.iter().any(|form| form == preferred) {
        return Err(NotationError::InvalidProfile(
            "preferred form must be declared exactly".to_owned(),
        ));
    }
    let mut seen = HashSet::new();
    for form in forms {
        validate_text("declared form", form)?;
        if !seen.insert(form) {
            return Err(NotationError::InvalidProfile(format!(
                "duplicate declared form `{form}`"
            )));
        }
    }
    Ok(())
}

fn validate_concept_id(value: &str) -> Result<(), NotationError> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(NotationError::InvalidProfile(format!(
            "invalid concept id `{value}`"
        )));
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), NotationError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES || value.contains('\0') {
        return Err(NotationError::InvalidProfile(format!(
            "{field} is empty or too large"
        )));
    }
    Ok(())
}

fn restrict_directory(path: &Path) -> Result<(), NotationError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(NotationError::Io)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), NotationError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(NotationError::Io)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum NotationError {
    #[error("notation settings path has no parent directory")]
    InvalidLocation,
    #[error("project identity is invalid")]
    InvalidProject,
    #[error("notation profile version {0} is unsupported")]
    UnsupportedVersion(u16),
    #[error("notation profile is invalid: {0}")]
    InvalidProfile(String),
    #[error("notation override references unknown concept `{0}`")]
    UnknownConcept(String),
    #[error("notation profile exceeds the size limit")]
    TooLarge,
    #[error("notation profile JSON is invalid: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("unable to serialize notation profile: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("notation profile filesystem operation failed: {0}")]
    Io(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn project_id() -> String {
        "a".repeat(64)
    }

    #[test]
    fn defaults_are_valid_and_have_explicit_forms() {
        let profile = default_profile();
        validate_profile(&profile).unwrap();
        assert!(
            profile
                .concepts
                .iter()
                .all(|concept| concept.declared_forms.contains(&concept.preferred_form))
        );
    }

    #[test]
    fn project_override_wins_and_reports_its_source() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("notation.json");
        let mut service = NotationService::load(path.clone()).unwrap();
        service
            .set_project_overrides(
                &project_id(),
                ProjectNotationOverrides {
                    version: 1,
                    concepts: vec![NotationConceptOverride {
                        concept_id: "adversary".to_owned(),
                        preferred_form: "\\mathsf{Adv}".to_owned(),
                        declared_forms: vec!["\\mathsf{Adv}".to_owned(), "\\mathcal{A}".to_owned()],
                    }],
                },
            )
            .unwrap();
        let reloaded = NotationService::load(path).unwrap();
        let effective = reloaded.effective(Some(&project_id())).unwrap();
        let adversary = effective
            .concepts
            .iter()
            .find(|item| item.id == "adversary")
            .unwrap();
        assert_eq!(adversary.preferred_form, "\\mathsf{Adv}");
        assert_eq!(adversary.source, NotationPreferenceSource::Project);
    }

    #[test]
    fn global_profile_round_trips_through_export_and_import() {
        let directory = tempdir().unwrap();
        let mut first = NotationService::load(directory.path().join("first.json")).unwrap();
        let mut profile = default_profile();
        profile.name = "My notation".to_owned();
        first.set_global(profile.clone()).unwrap();
        let exported = first.export_global().unwrap();
        let mut second = NotationService::load(directory.path().join("second.json")).unwrap();
        assert_eq!(second.import_global(&exported).unwrap(), profile);
        assert_eq!(second.effective(None).unwrap().name, "My notation");
    }

    #[test]
    fn rejects_unknown_fields_concepts_versions_and_ambiguous_forms() {
        let directory = tempdir().unwrap();
        let mut service = NotationService::load(directory.path().join("notation.json")).unwrap();
        assert!(matches!(
            service.import_global(r#"{"version":1,"name":"x","concepts":[],"extra":true}"#),
            Err(NotationError::InvalidJson(_))
        ));
        assert!(matches!(
            service.set_project_overrides(
                &project_id(),
                ProjectNotationOverrides {
                    version: 1,
                    concepts: vec![NotationConceptOverride {
                        concept_id: "unknown".to_owned(),
                        preferred_form: "x".to_owned(),
                        declared_forms: vec!["x".to_owned()]
                    }]
                }
            ),
            Err(NotationError::UnknownConcept(_))
        ));
        let mut profile = default_profile();
        profile.version = 2;
        assert!(matches!(
            service.set_global(profile),
            Err(NotationError::UnsupportedVersion(2))
        ));
        let mut profile = default_profile();
        profile.concepts[1]
            .declared_forms
            .push("\\lambda".to_owned());
        assert!(matches!(
            service.set_global(profile),
            Err(NotationError::InvalidProfile(_))
        ));
    }

    #[test]
    fn failed_persistence_restores_memory_state() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("notation.json");
        let mut service = NotationService::load(path.clone()).unwrap();
        fs::create_dir(&path).unwrap();
        let mut profile = default_profile();
        profile.name = "Cannot persist".to_owned();
        assert!(matches!(
            service.set_global(profile),
            Err(NotationError::Io(_))
        ));
        assert_eq!(
            service.effective(None).unwrap().name,
            "CrypTex cryptography defaults"
        );
    }
}
