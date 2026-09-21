use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const API_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ApiError {
    pub api_version: u16,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(transparent)]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct OperationId(pub String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct BackendEvent {
    pub api_version: u16,
    pub operation_id: OperationId,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct HealthResponse {
    pub api_version: u16,
    pub application: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ToolchainReadiness {
    pub api_version: u16,
    pub status: ToolchainStatus,
    pub toolchain_id: Option<String>,
    pub texlive_year: Option<u16>,
    pub texlive_revision: Option<u32>,
    pub platform: Option<String>,
    pub binaries: Vec<ToolchainBinary>,
    pub message: Option<String>,
    pub repairable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum ToolchainStatus {
    Missing,
    Ready,
    Corrupt,
    Incompatible,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ToolchainBinary {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum BuildReason {
    Save,
    Explicit,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum LatexEngine {
    PdfLatex,
    XeLatex,
    LuaLatex,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum ResolutionProvenance {
    ProjectPreference,
    MagicComment,
    Detected,
    Default,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct BuildConfiguration {
    pub api_version: u16,
    pub project_id: String,
    pub root_document: String,
    pub root_provenance: ResolutionProvenance,
    pub engine: LatexEngine,
    pub engine_provenance: ResolutionProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum BuildPermission {
    LatexmkRc,
    ShellEscape,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectPermission {
    pub permission: BuildPermission,
    pub allowed: bool,
    pub consequence: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectTrustState {
    pub api_version: u16,
    pub project_id: String,
    pub permissions: Vec<ProjectPermission>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectSummary {
    pub api_version: u16,
    pub project_id: String,
    pub name: String,
    pub canonical_root: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct FileTreePage {
    pub api_version: u16,
    pub directory: String,
    pub entries: Vec<FileTreeEntry>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct FileTreeEntry {
    pub name: String,
    pub relative_path: String,
    pub kind: FileTreeEntryKind,
    pub is_symlink: bool,
    pub accessible: bool,
    pub hidden: bool,
    pub generated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum FileTreeEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct TextDocument {
    pub api_version: u16,
    pub relative_path: String,
    pub text: String,
    pub fingerprint: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct WriteResult {
    pub api_version: u16,
    pub relative_path: String,
    pub fingerprint: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct ProjectFileChange {
    pub api_version: u16,
    pub project_id: String,
    pub relative_paths: Vec<String>,
    pub kind: ProjectFileChangeKind,
    pub self_write: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum ProjectFileChangeKind {
    Create,
    Modify,
    Remove,
    Rename,
    Rescan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct RootDocumentCandidates {
    pub api_version: u16,
    pub candidates: Vec<RootDocumentCandidate>,
    pub selected: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct RootDocumentCandidate {
    pub relative_path: String,
    pub reasons: Vec<RootDocumentReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum RootDocumentReason {
    Preferred,
    MagicRoot,
    DocumentClass,
    IncludesFiles,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct RecoveryInventory {
    pub api_version: u16,
    pub snapshots: Vec<RecoverySnapshot>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct RecoverySnapshot {
    pub api_version: u16,
    pub snapshot_version: u16,
    pub project_id: String,
    pub relative_path: String,
    pub text: String,
    pub base_fingerprint: String,
    pub revision: u64,
    pub updated_at_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_response_has_versioned_camel_case_wire_shape() {
        let response = HealthResponse {
            api_version: API_VERSION,
            application: "CrypTex".to_owned(),
            version: "test".to_owned(),
        };
        let value = serde_json::to_value(response).expect("health response serializes");
        assert_eq!(value["apiVersion"], API_VERSION);
        assert_eq!(value["application"], "CrypTex");
        assert!(value.get("api_version").is_none());
    }
}
