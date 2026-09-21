use crate::{
    api::{API_VERSION, BuildPermission, ProjectPermission, ProjectTrustState},
    project::ProjectId,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;

const TRUST_STORE_VERSION: u16 = 1;
const LATEXMK_RC_CONSEQUENCE: &str =
    "Allows this project’s .latexmkrc to execute Perl code during compilation.";
const SHELL_ESCAPE_CONSEQUENCE: &str =
    "Allows TeX commands in this project to start external programs during compilation.";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredPermissions {
    allow_latexmk_rc: bool,
    allow_shell_escape: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredTrust {
    version: u16,
    projects: HashMap<String, StoredPermissions>,
}

pub struct TrustService {
    path: PathBuf,
    projects: HashMap<String, StoredPermissions>,
}

impl TrustService {
    pub fn load(path: PathBuf) -> Result<Self, TrustError> {
        let projects = match fs::read(&path) {
            Ok(bytes) => {
                let stored: StoredTrust =
                    serde_json::from_slice(&bytes).map_err(TrustError::Invalid)?;
                if stored.version != TRUST_STORE_VERSION {
                    return Err(TrustError::UnsupportedVersion(stored.version));
                }
                for project_id in stored.projects.keys() {
                    ProjectId::parse(project_id).map_err(|_| TrustError::InvalidProject)?;
                }
                stored.projects
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(TrustError::Io(error)),
        };
        Ok(Self { path, projects })
    }

    pub fn state(&self, project_id: &str) -> Result<ProjectTrustState, TrustError> {
        let project_id = ProjectId::parse(project_id).map_err(|_| TrustError::InvalidProject)?;
        let permissions = self
            .projects
            .get(project_id.as_str())
            .cloned()
            .unwrap_or_default();
        Ok(to_state(project_id.as_str(), &permissions))
    }

    pub fn allows(
        &self,
        project_id: &str,
        permission: BuildPermission,
    ) -> Result<bool, TrustError> {
        let state = self.state(project_id)?;
        Ok(state
            .permissions
            .iter()
            .find(|entry| entry.permission == permission)
            .is_some_and(|entry| entry.allowed))
    }

    pub fn set(
        &mut self,
        project_id: &str,
        permission: BuildPermission,
        allowed: bool,
    ) -> Result<ProjectTrustState, TrustError> {
        let project_id = ProjectId::parse(project_id).map_err(|_| TrustError::InvalidProject)?;
        let key = project_id.as_str().to_owned();
        let previous = self.projects.get(&key).cloned();
        let entry = self.projects.entry(key.clone()).or_default();
        match permission {
            BuildPermission::LatexmkRc => entry.allow_latexmk_rc = allowed,
            BuildPermission::ShellEscape => entry.allow_shell_escape = allowed,
        }
        if !entry.allow_latexmk_rc && !entry.allow_shell_escape {
            self.projects.remove(&key);
        }
        if let Err(error) = self.persist() {
            restore(&mut self.projects, key, previous);
            return Err(error);
        }
        self.state(project_id.as_str())
    }

    pub fn revoke(&mut self, project_id: &str) -> Result<ProjectTrustState, TrustError> {
        let project_id = ProjectId::parse(project_id).map_err(|_| TrustError::InvalidProject)?;
        let key = project_id.as_str().to_owned();
        let previous = self.projects.remove(&key);
        if let Err(error) = self.persist() {
            restore(&mut self.projects, key, previous);
            return Err(error);
        }
        self.state(project_id.as_str())
    }

    fn persist(&self) -> Result<(), TrustError> {
        let parent = self.path.parent().ok_or(TrustError::InvalidLocation)?;
        fs::create_dir_all(parent).map_err(TrustError::Io)?;
        restrict_directory(parent)?;
        let bytes = serde_json::to_vec_pretty(&StoredTrust {
            version: TRUST_STORE_VERSION,
            projects: self.projects.clone(),
        })
        .map_err(TrustError::Serialize)?;
        let mut temporary = NamedTempFile::new_in(parent).map_err(TrustError::Io)?;
        restrict_file(temporary.as_file())?;
        temporary.write_all(&bytes).map_err(TrustError::Io)?;
        temporary.flush().map_err(TrustError::Io)?;
        temporary.as_file().sync_all().map_err(TrustError::Io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| TrustError::Io(error.error))?;
        sync_directory(parent)?;
        Ok(())
    }
}

fn to_state(project_id: &str, permissions: &StoredPermissions) -> ProjectTrustState {
    ProjectTrustState {
        api_version: API_VERSION,
        project_id: project_id.to_owned(),
        permissions: vec![
            ProjectPermission {
                permission: BuildPermission::LatexmkRc,
                allowed: permissions.allow_latexmk_rc,
                consequence: LATEXMK_RC_CONSEQUENCE.to_owned(),
            },
            ProjectPermission {
                permission: BuildPermission::ShellEscape,
                allowed: permissions.allow_shell_escape,
                consequence: SHELL_ESCAPE_CONSEQUENCE.to_owned(),
            },
        ],
    }
}

fn restore(
    projects: &mut HashMap<String, StoredPermissions>,
    key: String,
    previous: Option<StoredPermissions>,
) {
    if let Some(previous) = previous {
        projects.insert(key, previous);
    } else {
        projects.remove(&key);
    }
}

fn restrict_directory(path: &Path) -> Result<(), TrustError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(TrustError::Io)?;
    }
    Ok(())
}

fn restrict_file(file: &File) -> Result<(), TrustError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(TrustError::Io)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), TrustError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(TrustError::Io)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum TrustError {
    #[error("trust store path has no parent directory")]
    InvalidLocation,
    #[error("project identity is invalid")]
    InvalidProject,
    #[error("stored trust data is invalid: {0}")]
    Invalid(#[source] serde_json::Error),
    #[error("trust store version {0} is unsupported")]
    UnsupportedVersion(u16),
    #[error("unable to serialize trust data: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("trust store filesystem operation failed: {0}")]
    Io(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn permissions_are_independent_denied_by_default_and_persisted() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("trust.json");
        let project_id = "a".repeat(64);
        let mut service = TrustService::load(path.clone()).unwrap();
        assert!(
            !service
                .allows(&project_id, BuildPermission::LatexmkRc)
                .unwrap()
        );
        assert!(
            !service
                .allows(&project_id, BuildPermission::ShellEscape)
                .unwrap()
        );

        service
            .set(&project_id, BuildPermission::LatexmkRc, true)
            .unwrap();
        assert!(
            service
                .allows(&project_id, BuildPermission::LatexmkRc)
                .unwrap()
        );
        assert!(
            !service
                .allows(&project_id, BuildPermission::ShellEscape)
                .unwrap()
        );

        let loaded = TrustService::load(path).unwrap();
        assert!(
            loaded
                .allows(&project_id, BuildPermission::LatexmkRc)
                .unwrap()
        );
        assert!(
            !loaded
                .allows(&project_id, BuildPermission::ShellEscape)
                .unwrap()
        );
    }

    #[test]
    fn revocation_removes_every_permission_and_explains_consequences() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("trust.json");
        let project_id = "b".repeat(64);
        let mut service = TrustService::load(path.clone()).unwrap();
        service
            .set(&project_id, BuildPermission::LatexmkRc, true)
            .unwrap();
        service
            .set(&project_id, BuildPermission::ShellEscape, true)
            .unwrap();
        let state = service.revoke(&project_id).unwrap();
        assert!(
            state
                .permissions
                .iter()
                .all(|permission| !permission.allowed)
        );
        assert!(
            state
                .permissions
                .iter()
                .all(|permission| !permission.consequence.is_empty())
        );
        let json = fs::read_to_string(path).unwrap();
        assert!(!json.contains(&project_id));
    }

    #[test]
    fn rejects_invalid_identity_and_store_version() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("trust.json");
        let mut service = TrustService::load(path.clone()).unwrap();
        assert!(matches!(
            service.set("../project", BuildPermission::ShellEscape, true),
            Err(TrustError::InvalidProject)
        ));
        fs::write(&path, br#"{"version":2,"projects":{}}"#).unwrap();
        assert!(matches!(
            TrustService::load(path),
            Err(TrustError::UnsupportedVersion(2))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn persists_with_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/trust.json");
        let mut service = TrustService::load(path.clone()).unwrap();
        service
            .set(&"c".repeat(64), BuildPermission::ShellEscape, true)
            .unwrap();
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
