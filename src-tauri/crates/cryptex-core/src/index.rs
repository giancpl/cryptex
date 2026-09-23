use crate::{api::API_VERSION, project::ProjectPath};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

pub mod scanner;

pub const PROJECT_INDEX_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectIndex {
    pub api_version: u16,
    pub schema_version: u16,
    pub project_id: String,
    pub generation: u64,
    pub completeness: IndexCompleteness,
    pub scanner_limits: ScannerLimits,
    pub files: Vec<IndexedFile>,
    pub issues: Vec<IndexIssue>,
}

impl ProjectIndex {
    pub fn empty(project_id: String, generation: u64) -> Self {
        Self {
            api_version: API_VERSION,
            schema_version: PROJECT_INDEX_SCHEMA_VERSION,
            project_id,
            generation,
            completeness: IndexCompleteness::BestEffort,
            scanner_limits: ScannerLimits::default(),
            files: Vec::new(),
            issues: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), IndexSchemaError> {
        if self.api_version != API_VERSION {
            return Err(IndexSchemaError::UnsupportedApiVersion(self.api_version));
        }
        if self.schema_version != PROJECT_INDEX_SCHEMA_VERSION {
            return Err(IndexSchemaError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.project_id.is_empty() {
            return Err(IndexSchemaError::EmptyProjectId);
        }
        self.scanner_limits.validate()?;
        for file in &self.files {
            file.validate()?;
        }
        for issue in &self.issues {
            issue.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum IndexCompleteness {
    BestEffort,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ScannerLimits {
    pub max_project_files: u32,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_records_per_file: u32,
    pub max_brace_depth: u16,
    pub max_command_bytes: u32,
}

impl Default for ScannerLimits {
    fn default() -> Self {
        Self {
            max_project_files: 10_000,
            max_total_bytes: 256 * 1024 * 1024,
            max_file_bytes: 5 * 1024 * 1024,
            max_records_per_file: 50_000,
            max_brace_depth: 256,
            max_command_bytes: 4_096,
        }
    }
}

impl ScannerLimits {
    fn validate(&self) -> Result<(), IndexSchemaError> {
        if self.max_project_files == 0
            || self.max_total_bytes == 0
            || self.max_file_bytes == 0
            || self.max_records_per_file == 0
            || self.max_brace_depth == 0
            || self.max_command_bytes == 0
        {
            return Err(IndexSchemaError::InvalidScannerLimits);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct IndexedFile {
    pub relative_path: String,
    pub fingerprint: String,
    pub status: FileIndexStatus,
    pub records: Vec<IndexRecord>,
    pub issues: Vec<IndexIssue>,
}

impl IndexedFile {
    fn validate(&self) -> Result<(), IndexSchemaError> {
        if self.relative_path.is_empty() || ProjectPath::parse(&self.relative_path).is_err() {
            return Err(IndexSchemaError::InvalidRelativePath(
                self.relative_path.clone(),
            ));
        }
        if self.fingerprint.len() != 64
            || !self
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(IndexSchemaError::EmptyFingerprint(
                self.relative_path.clone(),
            ));
        }
        for record in &self.records {
            record.validate(&self.relative_path)?;
        }
        for issue in &self.issues {
            issue.validate()?;
            if issue.relative_path.as_deref() != Some(self.relative_path.as_str()) {
                return Err(IndexSchemaError::MismatchedIssuePath);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum FileIndexStatus {
    Complete,
    Partial,
    Skipped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct IndexRecord {
    pub kind: IndexRecordKind,
    pub name: String,
    pub target: Option<String>,
    pub range: IndexSourceRange,
    pub confidence: IndexConfidence,
    pub provenance: IndexProvenance,
}

impl IndexRecord {
    fn validate(&self, relative_path: &str) -> Result<(), IndexSchemaError> {
        if self.name.is_empty() {
            return Err(IndexSchemaError::EmptyRecordName(relative_path.to_owned()));
        }
        self.range.validate()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum IndexRecordKind {
    Section,
    Label,
    Reference,
    Citation,
    Environment,
    MacroDefinition,
    MacroUsage,
    Include,
    Package,
    Cryptocode,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum IndexConfidence {
    Exact,
    Recovered,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum IndexProvenance {
    Lexical,
    ErrorRecovery,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct IndexSourceRange {
    pub start_byte: u64,
    pub end_byte: u64,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl IndexSourceRange {
    fn validate(&self) -> Result<(), IndexSchemaError> {
        if self.start_byte > self.end_byte
            || self.start_line == 0
            || self.start_column == 0
            || self.end_line == 0
            || self.end_column == 0
            || (self.start_line, self.start_column) > (self.end_line, self.end_column)
        {
            return Err(IndexSchemaError::InvalidSourceRange);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct IndexIssue {
    pub code: IndexIssueCode,
    pub message: String,
    pub relative_path: Option<String>,
    pub range: Option<IndexSourceRange>,
}

impl IndexIssue {
    fn validate(&self) -> Result<(), IndexSchemaError> {
        if self.message.is_empty() {
            return Err(IndexSchemaError::EmptyIssueMessage);
        }
        if let Some(path) = &self.relative_path
            && (path.is_empty() || ProjectPath::parse(path).is_err())
        {
            return Err(IndexSchemaError::InvalidRelativePath(path.clone()));
        }
        if let Some(range) = self.range {
            range.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum IndexIssueCode {
    FileTooLarge,
    RecordLimitReached,
    BraceDepthLimitReached,
    CommandTooLong,
    MalformedInput,
    UnsupportedEncoding,
    ReadFailed,
    MissingInclude,
    IncludeCycle,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum IndexSchemaError {
    #[error("unsupported API version {0}")]
    UnsupportedApiVersion(u16),
    #[error("unsupported project index schema version {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("project identity is empty")]
    EmptyProjectId,
    #[error("scanner limits must be nonzero")]
    InvalidScannerLimits,
    #[error("invalid project-relative path: {0}")]
    InvalidRelativePath(String),
    #[error("fingerprint is not a lowercase SHA-256 digest for {0}")]
    EmptyFingerprint(String),
    #[error("record name is empty in {0}")]
    EmptyRecordName(String),
    #[error("source range is invalid")]
    InvalidSourceRange,
    #[error("index issue message is empty")]
    EmptyIssueMessage,
    #[error("file-local issue path does not match its file")]
    MismatchedIssuePath,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range() -> IndexSourceRange {
        IndexSourceRange {
            start_byte: 10,
            end_byte: 20,
            start_line: 2,
            start_column: 1,
            end_line: 2,
            end_column: 11,
        }
    }

    #[test]
    fn empty_index_is_explicitly_best_effort_and_versioned() {
        let index = ProjectIndex::empty("a".repeat(64), 7);
        assert_eq!(index.api_version, API_VERSION);
        assert_eq!(index.schema_version, PROJECT_INDEX_SCHEMA_VERSION);
        assert_eq!(index.completeness, IndexCompleteness::BestEffort);
        assert_eq!(index.scanner_limits, ScannerLimits::default());
        index.validate().unwrap();

        let json = serde_json::to_value(index).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["completeness"], "bestEffort");
        assert_eq!(json["scannerLimits"]["maxBraceDepth"], 256);
    }

    #[test]
    fn schema_represents_every_required_record_kind_with_provenance() {
        let kinds = [
            IndexRecordKind::Section,
            IndexRecordKind::Label,
            IndexRecordKind::Reference,
            IndexRecordKind::Citation,
            IndexRecordKind::Environment,
            IndexRecordKind::MacroDefinition,
            IndexRecordKind::MacroUsage,
            IndexRecordKind::Include,
            IndexRecordKind::Package,
            IndexRecordKind::Cryptocode,
        ];
        let records = kinds
            .into_iter()
            .map(|kind| IndexRecord {
                kind,
                name: "value".to_owned(),
                target: None,
                range: range(),
                confidence: IndexConfidence::Exact,
                provenance: IndexProvenance::Lexical,
            })
            .collect();
        let index = ProjectIndex {
            files: vec![IndexedFile {
                relative_path: "main.tex".to_owned(),
                fingerprint: "f".repeat(64),
                status: FileIndexStatus::Partial,
                records,
                issues: Vec::new(),
            }],
            ..ProjectIndex::empty("a".repeat(64), 1)
        };
        index.validate().unwrap();
        assert_eq!(index.files[0].records.len(), 10);
    }

    #[test]
    fn rejects_invalid_ranges_paths_limits_and_file_issue_mismatches() {
        let mut index = ProjectIndex::empty("project".to_owned(), 1);
        index.scanner_limits.max_brace_depth = 0;
        assert_eq!(
            index.validate(),
            Err(IndexSchemaError::InvalidScannerLimits)
        );

        let invalid_range = IndexSourceRange {
            start_byte: 20,
            end_byte: 10,
            ..range()
        };
        assert_eq!(
            invalid_range.validate(),
            Err(IndexSchemaError::InvalidSourceRange)
        );

        let file = IndexedFile {
            relative_path: "../outside.tex".to_owned(),
            fingerprint: "f".repeat(64),
            status: FileIndexStatus::Skipped,
            records: Vec::new(),
            issues: Vec::new(),
        };
        assert!(matches!(
            file.validate(),
            Err(IndexSchemaError::InvalidRelativePath(_))
        ));

        let issue = IndexIssue {
            code: IndexIssueCode::MalformedInput,
            message: "recovered".to_owned(),
            relative_path: Some("other.tex".to_owned()),
            range: None,
        };
        let file = IndexedFile {
            relative_path: "main.tex".to_owned(),
            fingerprint: "f".repeat(64),
            status: FileIndexStatus::Partial,
            records: Vec::new(),
            issues: vec![issue],
        };
        assert_eq!(file.validate(), Err(IndexSchemaError::MismatchedIssuePath));
    }

    #[test]
    fn round_trip_preserves_unknown_or_partial_evidence_without_claiming_semantics() {
        let index = ProjectIndex {
            files: vec![IndexedFile {
                relative_path: "main.tex".to_owned(),
                fingerprint: "f".repeat(64),
                status: FileIndexStatus::Partial,
                records: vec![IndexRecord {
                    kind: IndexRecordKind::Cryptocode,
                    name: "game".to_owned(),
                    target: Some("unknown-signature".to_owned()),
                    range: range(),
                    confidence: IndexConfidence::Recovered,
                    provenance: IndexProvenance::ErrorRecovery,
                }],
                issues: vec![IndexIssue {
                    code: IndexIssueCode::MalformedInput,
                    message: "unbalanced group; later records may be missing".to_owned(),
                    relative_path: Some("main.tex".to_owned()),
                    range: Some(range()),
                }],
            }],
            ..ProjectIndex::empty("a".repeat(64), 3)
        };
        let encoded = serde_json::to_vec(&index).unwrap();
        let decoded: ProjectIndex = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, index);
        decoded.validate().unwrap();
    }
}
