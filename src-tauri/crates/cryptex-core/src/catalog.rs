use crate::api::API_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use ts_rs::TS;

pub const COMMAND_CATALOG_SCHEMA_VERSION: u16 = 1;
const BUNDLED_CATALOG: &str = include_str!("../assets/command-catalog-v1.json");
const MAX_ENTRIES: usize = 10_000;
const MAX_FIELD_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct CommandCatalog {
    pub api_version: u16,
    pub schema_version: u16,
    pub catalog_version: String,
    pub entries: Vec<CommandEntry>,
}

impl CommandCatalog {
    pub fn bundled() -> Result<Self, CatalogLoadError> {
        let catalog: Self = serde_json::from_str(BUNDLED_CATALOG)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.api_version != API_VERSION {
            return Err(CatalogError::UnsupportedApiVersion(self.api_version));
        }
        if self.schema_version != COMMAND_CATALOG_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchemaVersion(self.schema_version));
        }
        required("catalogVersion", &self.catalog_version)?;
        if self.entries.len() > MAX_ENTRIES {
            return Err(CatalogError::TooManyEntries);
        }
        let mut ids = HashSet::new();
        let mut variants = HashSet::new();
        for entry in &self.entries {
            entry.validate()?;
            if !ids.insert(entry.id.to_ascii_lowercase()) {
                return Err(CatalogError::DuplicateId(entry.id.clone()));
            }
            let mut contexts = entry.contexts.clone();
            contexts.sort_unstable();
            let key = (
                entry.command.to_ascii_lowercase(),
                entry.signature.to_ascii_lowercase(),
                contexts,
            );
            if !variants.insert(key) {
                return Err(CatalogError::DuplicateVariant(entry.id.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CatalogLoadError {
    #[error("bundled command catalog is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("bundled command catalog failed validation: {0}")]
    Validation(#[from] CatalogError),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct CommandEntry {
    pub id: String,
    pub command: String,
    pub display_name: String,
    pub summary: String,
    pub concepts: Vec<String>,
    pub synonyms: Vec<String>,
    pub requirements: Vec<PackageRequirement>,
    pub signature: String,
    pub snippet: String,
    pub examples: Vec<CommandExample>,
    pub documentation_url: String,
    pub contexts: Vec<CommandContext>,
    pub provenance: CatalogProvenance,
}

impl CommandEntry {
    fn validate(&self) -> Result<(), CatalogError> {
        for (field, value) in [
            ("id", self.id.as_str()),
            ("command", self.command.as_str()),
            ("displayName", self.display_name.as_str()),
            ("summary", self.summary.as_str()),
            ("signature", self.signature.as_str()),
            ("snippet", self.snippet.as_str()),
        ] {
            required(field, value)?;
        }
        if !self.id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        }) {
            return Err(CatalogError::InvalidId(self.id.clone()));
        }
        if !self.command.starts_with('\\')
            || self.command.chars().any(char::is_whitespace)
            || self.command.contains(['\n', '\r'])
        {
            return Err(CatalogError::InvalidCommand(self.id.clone()));
        }
        if self.concepts.is_empty() {
            return Err(CatalogError::MissingConcept(self.id.clone()));
        }
        unique_terms(&self.id, "concept", &self.concepts)?;
        unique_terms(&self.id, "synonym", &self.synonyms)?;
        if self.contexts.is_empty() {
            return Err(CatalogError::MissingContext(self.id.clone()));
        }
        let contexts: HashSet<_> = self.contexts.iter().copied().collect();
        if contexts.len() != self.contexts.len() {
            return Err(CatalogError::DuplicateContext(self.id.clone()));
        }
        if contexts.contains(&CommandContext::Any) && contexts.len() > 1 {
            return Err(CatalogError::AnyContextCombined(self.id.clone()));
        }
        for requirement in &self.requirements {
            requirement.validate(&self.id)?;
        }
        for example in &self.examples {
            example.validate(&self.id)?;
        }
        https_url(&self.id, "documentationUrl", &self.documentation_url)?;
        self.provenance.validate(&self.id)?;
        validate_snippet(&self.id, &self.snippet)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct PackageRequirement {
    pub package: String,
    pub version_requirement: Option<String>,
}

impl PackageRequirement {
    fn validate(&self, entry: &str) -> Result<(), CatalogError> {
        required("package", &self.package)?;
        if !self
            .package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(CatalogError::InvalidPackage(entry.to_owned()));
        }
        if let Some(version) = &self.version_requirement {
            required("versionRequirement", version)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct CommandExample {
    pub title: String,
    pub latex: String,
    pub explanation: String,
}

impl CommandExample {
    fn validate(&self, entry: &str) -> Result<(), CatalogError> {
        required("example.title", &self.title)?;
        required("example.latex", &self.latex)?;
        required("example.explanation", &self.explanation)
            .map_err(|_| CatalogError::InvalidExample(entry.to_owned()))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum CommandContext {
    Any,
    Text,
    Math,
    Preamble,
    Environment,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct CatalogProvenance {
    pub source_title: String,
    pub source_url: String,
    pub source_version: String,
}

impl CatalogProvenance {
    fn validate(&self, entry: &str) -> Result<(), CatalogError> {
        required("provenance.sourceTitle", &self.source_title)?;
        required("provenance.sourceVersion", &self.source_version)?;
        https_url(entry, "provenance.sourceUrl", &self.source_url)
    }
}

fn required(field: &'static str, value: &str) -> Result<(), CatalogError> {
    if value.trim().is_empty() || value.len() > MAX_FIELD_BYTES || value.contains('\0') {
        Err(CatalogError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn unique_terms(entry: &str, kind: &'static str, values: &[String]) -> Result<(), CatalogError> {
    let mut seen = HashSet::new();
    for value in values {
        required(kind, value)?;
        if !seen.insert(value.trim().to_lowercase()) {
            return Err(CatalogError::DuplicateTerm {
                entry: entry.to_owned(),
                kind,
            });
        }
    }
    Ok(())
}

fn https_url(entry: &str, field: &'static str, value: &str) -> Result<(), CatalogError> {
    required(field, value)?;
    if !value.starts_with("https://") || value.contains(char::is_whitespace) {
        return Err(CatalogError::InvalidUrl {
            entry: entry.to_owned(),
            field,
        });
    }
    Ok(())
}

fn validate_snippet(entry: &str, snippet: &str) -> Result<(), CatalogError> {
    let bytes = snippet.as_bytes();
    let mut cursor = 0;
    let mut positions = HashSet::new();
    let mut final_cursor = false;
    while cursor < bytes.len() {
        if bytes[cursor] != b'$' {
            cursor += 1;
            continue;
        }
        if bytes.get(cursor + 1) == Some(&b'0') {
            if final_cursor {
                return Err(CatalogError::InvalidSnippet(entry.to_owned()));
            }
            final_cursor = true;
            cursor += 2;
            continue;
        }
        if bytes.get(cursor + 1) != Some(&b'{') {
            cursor += 1;
            continue;
        }
        let Some(offset) = bytes[cursor + 2..].iter().position(|byte| *byte == b'}') else {
            return Err(CatalogError::InvalidSnippet(entry.to_owned()));
        };
        let close = cursor + 2 + offset;
        let body = std::str::from_utf8(&bytes[cursor + 2..close])
            .map_err(|_| CatalogError::InvalidSnippet(entry.to_owned()))?;
        if body.contains("${") {
            return Err(CatalogError::InvalidSnippet(entry.to_owned()));
        }
        let Some((number, default)) = body.split_once(':') else {
            return Err(CatalogError::InvalidSnippet(entry.to_owned()));
        };
        let number = number
            .parse::<u16>()
            .ok()
            .filter(|number| *number > 0)
            .ok_or_else(|| CatalogError::InvalidSnippet(entry.to_owned()))?;
        if default.is_empty() || !positions.insert(number) {
            return Err(CatalogError::InvalidSnippet(entry.to_owned()));
        }
        cursor = close + 1;
    }
    Ok(())
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CatalogError {
    #[error("unsupported API version {0}")]
    UnsupportedApiVersion(u16),
    #[error("unsupported command catalog schema version {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("catalog contains too many entries")]
    TooManyEntries,
    #[error("catalog field is empty, oversized, or contains NUL: {0}")]
    InvalidField(&'static str),
    #[error("catalog entry id is invalid: {0}")]
    InvalidId(String),
    #[error("duplicate catalog entry id: {0}")]
    DuplicateId(String),
    #[error("duplicate command/signature/context variant: {0}")]
    DuplicateVariant(String),
    #[error("catalog command is invalid: {0}")]
    InvalidCommand(String),
    #[error("catalog entry has no concepts: {0}")]
    MissingConcept(String),
    #[error("catalog entry has no contexts: {0}")]
    MissingContext(String),
    #[error("catalog entry repeats a context: {0}")]
    DuplicateContext(String),
    #[error("catalog entry combines any with specific contexts: {0}")]
    AnyContextCombined(String),
    #[error("duplicate {kind} in catalog entry {entry}")]
    DuplicateTerm { entry: String, kind: &'static str },
    #[error("package requirement is invalid in catalog entry {0}")]
    InvalidPackage(String),
    #[error("example is invalid in catalog entry {0}")]
    InvalidExample(String),
    #[error("URL field {field} is invalid in catalog entry {entry}")]
    InvalidUrl { entry: String, field: &'static str },
    #[error("snippet placeholders are malformed or duplicated in catalog entry {0}")]
    InvalidSnippet(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> CommandEntry {
        CommandEntry {
            id: id.to_owned(),
            command: "\\sample".to_owned(),
            display_name: "Sample".to_owned(),
            summary: "Samples a value.".to_owned(),
            concepts: vec!["random sampling".to_owned()],
            synonyms: vec!["draw uniformly".to_owned()],
            requirements: vec![PackageRequirement {
                package: "cryptocode".to_owned(),
                version_requirement: Some(">=0.44".to_owned()),
            }],
            signature: "\\sample".to_owned(),
            snippet: "\\sample $0".to_owned(),
            examples: vec![CommandExample {
                title: "Assignment".to_owned(),
                latex: "x \\sample \\{0,1\\}".to_owned(),
                explanation: "Draw a bit.".to_owned(),
            }],
            documentation_url: "https://example.test/command".to_owned(),
            contexts: vec![CommandContext::Math],
            provenance: CatalogProvenance {
                source_title: "Package manual".to_owned(),
                source_url: "https://example.test/manual".to_owned(),
                source_version: "0.44".to_owned(),
            },
        }
    }

    fn catalog(entries: Vec<CommandEntry>) -> CommandCatalog {
        CommandCatalog {
            api_version: API_VERSION,
            schema_version: COMMAND_CATALOG_SCHEMA_VERSION,
            catalog_version: "2026.1".to_owned(),
            entries,
        }
    }

    #[test]
    fn valid_catalog_round_trips_with_versioned_camel_case_shape() {
        let value = catalog(vec![entry("cryptocode.sample")]);
        value.validate().unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["entries"][0]["contexts"][0], "math");
        assert_eq!(
            serde_json::from_value::<CommandCatalog>(json).unwrap(),
            value
        );
    }

    #[test]
    fn rejects_duplicates_and_incomplete_provenance() {
        let duplicate = entry("cryptocode.sample");
        assert_eq!(
            catalog(vec![duplicate.clone(), duplicate]).validate(),
            Err(CatalogError::DuplicateId("cryptocode.sample".to_owned()))
        );
        let mut second = entry("cryptocode.alias");
        second.display_name = "Alias".to_owned();
        assert_eq!(
            catalog(vec![entry("cryptocode.sample"), second]).validate(),
            Err(CatalogError::DuplicateVariant(
                "cryptocode.alias".to_owned()
            ))
        );
        let mut value = entry("cryptocode.sample");
        value.provenance.source_version.clear();
        assert_eq!(
            catalog(vec![value]).validate(),
            Err(CatalogError::InvalidField("provenance.sourceVersion"))
        );
    }

    #[test]
    fn rejects_insecure_urls_invalid_packages_contexts_and_terms() {
        let mut value = entry("cryptocode.sample");
        value.documentation_url = "http://example.test".to_owned();
        assert!(matches!(
            catalog(vec![value]).validate(),
            Err(CatalogError::InvalidUrl { .. })
        ));
        let mut value = entry("cryptocode.sample");
        value.requirements[0].package = "../package".to_owned();
        assert!(matches!(
            catalog(vec![value]).validate(),
            Err(CatalogError::InvalidPackage(_))
        ));
        let mut value = entry("cryptocode.sample");
        value.contexts = vec![CommandContext::Any, CommandContext::Math];
        assert!(matches!(
            catalog(vec![value]).validate(),
            Err(CatalogError::AnyContextCombined(_))
        ));
        let mut value = entry("cryptocode.sample");
        value.synonyms.push("DRAW UNIFORMLY".to_owned());
        assert!(matches!(
            catalog(vec![value]).validate(),
            Err(CatalogError::DuplicateTerm { .. })
        ));
    }

    #[test]
    fn validates_placeholders_but_allows_literal_math_dollars() {
        let invalid = [
            ["$", "{name}"].concat(),
            ["$", "{0:value}"].concat(),
            ["$", "{1:}"].concat(),
            ["$", "{1:a}$", "{1:b}"].concat(),
            ["$", "{1:a"].concat(),
            ["$", "{1:a$", "{2:b}}"].concat(),
        ];
        for snippet in invalid {
            let mut value = entry("cryptocode.sample");
            value.snippet = snippet;
            assert!(matches!(
                catalog(vec![value]).validate(),
                Err(CatalogError::InvalidSnippet(_))
            ));
        }
        let mut value = entry("latex.display-math");
        value.command = "\\[".to_owned();
        value.signature = "\\[...\\]".to_owned();
        value.snippet = ["\\[\n  $", "{1:expression with $x$}\n\\]\n$0"].concat();
        value.contexts = vec![CommandContext::Text];
        catalog(vec![value]).validate().unwrap();
    }

    #[test]
    fn bundled_catalog_is_valid_versioned_and_curated() {
        let catalog = CommandCatalog::bundled().unwrap();
        assert_eq!(catalog.catalog_version, "2026.1");
        assert!(catalog.entries.len() >= 16);
        assert!(
            catalog
                .entries
                .iter()
                .any(|entry| entry.id == "latex.section")
        );
        assert!(
            catalog
                .entries
                .iter()
                .any(|entry| entry.id == "cryptocode.pseudocode")
        );

        let cryptocode = catalog
            .entries
            .iter()
            .filter(|entry| entry.id.starts_with("cryptocode."))
            .collect::<Vec<_>>();
        assert!(cryptocode.len() >= 8);
        assert!(cryptocode.iter().all(|entry| {
            entry.requirements.iter().any(|requirement| {
                requirement.package == "cryptocode"
                    && requirement.version_requirement.as_deref() == Some("=0.44")
            }) && entry.provenance.source_version == "0.44"
                && entry.provenance.source_url
                    == "https://mirrors.ctan.org/macros/latex/contrib/cryptocode/cryptocode.pdf"
        }));
    }
}
