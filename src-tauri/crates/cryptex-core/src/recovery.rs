use crate::{
    api::{API_VERSION, RecoveryInventory, RecoverySnapshot},
    project::{MAX_TEXT_FILE_BYTES, ProjectId, ProjectPath, ProjectPathError},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;
use thiserror::Error;

const MAX_RECOVERY_SNAPSHOTS: usize = 1_000;
const SNAPSHOT_VERSION: u16 = 1;

pub struct RecoveryService {
    root: PathBuf,
}

impl RecoveryService {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn store(
        &self,
        project_id: &str,
        relative_path: &str,
        text: &str,
        base_fingerprint: &str,
        revision: u64,
    ) -> Result<RecoverySnapshot, RecoveryError> {
        if text.len() as u64 > MAX_TEXT_FILE_BYTES {
            return Err(RecoveryError::TooLarge);
        }
        let (project_id, relative_path) = validate_identity(project_id, relative_path)?;
        if !valid_fingerprint(base_fingerprint) {
            return Err(RecoveryError::InvalidFingerprint);
        }
        let snapshot = RecoverySnapshot {
            api_version: API_VERSION,
            snapshot_version: SNAPSHOT_VERSION,
            project_id: project_id.as_str().to_owned(),
            relative_path: relative_path.to_string_lossy().into_owned(),
            text: text.to_owned(),
            base_fingerprint: base_fingerprint.to_ascii_lowercase(),
            revision,
            updated_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        };
        let directory = self.project_directory(&project_id);
        fs::create_dir_all(&directory).map_err(RecoveryError::Io)?;
        restrict_directory(&self.root)?;
        restrict_directory(&directory)?;
        let bytes = serde_json::to_vec(&snapshot).map_err(RecoveryError::Serialize)?;
        let mut temporary = NamedTempFile::new_in(&directory).map_err(RecoveryError::Io)?;
        temporary.write_all(&bytes).map_err(RecoveryError::Io)?;
        temporary.flush().map_err(RecoveryError::Io)?;
        temporary.as_file().sync_all().map_err(RecoveryError::Io)?;
        restrict_file(temporary.as_file())?;
        temporary
            .persist(self.snapshot_path(&project_id, &relative_path))
            .map_err(|error| RecoveryError::Io(error.error))?;
        sync_directory(&directory)?;
        Ok(snapshot)
    }

    pub fn list(&self, project_id: &str) -> Result<RecoveryInventory, RecoveryError> {
        let project_id = ProjectId::parse(project_id).map_err(|_| RecoveryError::InvalidProject)?;
        let directory = self.project_directory(&project_id);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RecoveryInventory {
                    api_version: API_VERSION,
                    snapshots: Vec::new(),
                    warnings: Vec::new(),
                });
            }
            Err(error) => return Err(RecoveryError::Io(error)),
        };
        let mut snapshots = Vec::new();
        let mut warnings = Vec::new();
        for entry in entries.take(MAX_RECOVERY_SNAPSHOTS) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warnings.push(error.to_string());
                    continue;
                }
            };
            let bytes = match fs::read(entry.path()) {
                Ok(bytes) => bytes,
                Err(error) => {
                    warnings.push(error.to_string());
                    continue;
                }
            };
            match serde_json::from_slice::<RecoverySnapshot>(&bytes) {
                Ok(snapshot)
                    if snapshot.api_version == API_VERSION
                        && snapshot.snapshot_version == SNAPSHOT_VERSION
                        && snapshot.project_id == project_id.as_str()
                        && snapshot.text.len() as u64 <= MAX_TEXT_FILE_BYTES
                        && valid_fingerprint(&snapshot.base_fingerprint)
                        && validate_identity(&snapshot.project_id, &snapshot.relative_path)
                            .is_ok() =>
                {
                    snapshots.push(snapshot);
                }
                Ok(_) => warnings.push("Ignored incompatible recovery snapshot".to_owned()),
                Err(_) => warnings.push("Ignored corrupt recovery snapshot".to_owned()),
            }
        }
        snapshots.sort_by(|left, right| {
            right
                .updated_at_ms
                .cmp(&left.updated_at_ms)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        Ok(RecoveryInventory {
            api_version: API_VERSION,
            snapshots,
            warnings,
        })
    }

    pub fn delete(&self, project_id: &str, relative_path: &str) -> Result<(), RecoveryError> {
        let (project_id, relative_path) = validate_identity(project_id, relative_path)?;
        let path = self.snapshot_path(&project_id, &relative_path);
        match fs::remove_file(path) {
            Ok(()) => sync_directory(&self.project_directory(&project_id)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RecoveryError::Io(error)),
        }
    }

    fn project_directory(&self, project_id: &ProjectId) -> PathBuf {
        self.root.join(project_id.as_str())
    }

    fn snapshot_path(&self, project_id: &ProjectId, relative_path: &Path) -> PathBuf {
        let digest = Sha256::digest(relative_path.as_os_str().as_encoded_bytes());
        self.project_directory(project_id)
            .join(format!("{digest:x}.json"))
    }
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_identity(
    project_id: &str,
    relative_path: &str,
) -> Result<(ProjectId, PathBuf), RecoveryError> {
    let project_id = ProjectId::parse(project_id).map_err(|_| RecoveryError::InvalidProject)?;
    let path = ProjectPath::parse(relative_path).map_err(RecoveryError::UnsafePath)?;
    if path.as_path().as_os_str().is_empty() {
        return Err(RecoveryError::EmptyPath);
    }
    Ok((project_id, path.as_path().to_path_buf()))
}

fn restrict_directory(path: &Path) -> Result<(), RecoveryError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(RecoveryError::Io)?;
    }
    Ok(())
}

fn restrict_file(file: &File) -> Result<(), RecoveryError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(RecoveryError::Io)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), RecoveryError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(RecoveryError::Io)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("project identity is invalid")]
    InvalidProject,
    #[error("recovery path cannot be empty")]
    EmptyPath,
    #[error("recovery path is unsafe: {0}")]
    UnsafePath(#[source] ProjectPathError),
    #[error("recovery fingerprint is invalid")]
    InvalidFingerprint,
    #[error("recovery snapshot exceeds the text editing limit")]
    TooLarge,
    #[error("unable to serialize recovery snapshot: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("recovery filesystem operation failed: {0}")]
    Io(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn snapshots_round_trip_and_delete_without_source_files() {
        let directory = tempdir().expect("recovery directory");
        let service = RecoveryService::new(directory.path().join("recovery"));
        let project_id = "a".repeat(64);
        service
            .store(&project_id, "renamed/main.tex", "draft", &"b".repeat(64), 7)
            .expect("store snapshot");

        let restarted = RecoveryService::new(directory.path().join("recovery"));
        let inventory = restarted.list(&project_id).expect("list after restart");
        assert_eq!(inventory.snapshots.len(), 1);
        assert_eq!(inventory.snapshots[0].text, "draft");
        assert_eq!(inventory.snapshots[0].revision, 7);

        restarted
            .delete(&project_id, "renamed/main.tex")
            .expect("delete snapshot");
        assert!(
            restarted
                .list(&project_id)
                .expect("list after delete")
                .snapshots
                .is_empty()
        );
    }

    #[test]
    fn corrupt_snapshots_are_reported_without_hiding_valid_ones() {
        let directory = tempdir().expect("recovery directory");
        let root = directory.path().join("recovery");
        let service = RecoveryService::new(root.clone());
        let project_id = "c".repeat(64);
        service
            .store(&project_id, "main.tex", "draft", &"d".repeat(64), 1)
            .expect("store snapshot");
        fs::write(root.join(&project_id).join("corrupt.json"), b"not json")
            .expect("corrupt fixture");

        let inventory = service.list(&project_id).expect("list snapshots");
        assert_eq!(inventory.snapshots.len(), 1);
        assert_eq!(
            inventory.warnings,
            vec!["Ignored corrupt recovery snapshot"]
        );
    }

    #[test]
    fn rejects_unsafe_paths_and_separates_project_identities() {
        let directory = tempdir().expect("recovery directory");
        let service = RecoveryService::new(directory.path().join("recovery"));
        let first = "e".repeat(64);
        let second = "f".repeat(64);
        assert!(matches!(
            service.store(&first, "../main.tex", "draft", &"0".repeat(64), 1),
            Err(RecoveryError::UnsafePath(_))
        ));
        service
            .store(&first, "main.tex", "draft", &"0".repeat(64), 1)
            .expect("store first project");
        assert!(
            service
                .list(&second)
                .expect("list second project")
                .snapshots
                .is_empty()
        );
    }
}
