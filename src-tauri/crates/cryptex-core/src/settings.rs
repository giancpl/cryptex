use crate::{
    api::LatexEngine,
    project::{ProjectId, ProjectPath, ProjectPathError},
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

const SETTINGS_VERSION: u16 = 1;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredRootPreferences {
    version: u16,
    projects: HashMap<String, String>,
}

pub struct RootPreferences {
    path: PathBuf,
    projects: HashMap<String, String>,
}

impl RootPreferences {
    pub fn load(path: PathBuf) -> Result<Self, SettingsError> {
        let projects = match fs::read(&path) {
            Ok(bytes) => {
                let stored: StoredRootPreferences =
                    serde_json::from_slice(&bytes).map_err(SettingsError::Invalid)?;
                if stored.version != SETTINGS_VERSION {
                    return Err(SettingsError::UnsupportedVersion(stored.version));
                }
                stored.projects
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(SettingsError::Io(error)),
        };
        Ok(Self { path, projects })
    }

    pub fn get(&self, project_id: &str) -> Option<&str> {
        self.projects.get(project_id).map(String::as_str)
    }

    pub fn set(&mut self, project_id: &str, relative_path: &str) -> Result<(), SettingsError> {
        let project_id = ProjectId::parse(project_id).map_err(|_| SettingsError::InvalidProject)?;
        let path = ProjectPath::parse(relative_path).map_err(SettingsError::UnsafePath)?;
        if path.as_path().as_os_str().is_empty() {
            return Err(SettingsError::UnsafePath(ProjectPathError::Traversal));
        }
        let key = project_id.as_str().to_owned();
        let previous = self
            .projects
            .insert(key.clone(), path.as_path().to_string_lossy().into_owned());
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

    fn persist(&self) -> Result<(), SettingsError> {
        let parent = self.path.parent().ok_or(SettingsError::InvalidLocation)?;
        fs::create_dir_all(parent).map_err(SettingsError::Io)?;
        restrict_directory(parent)?;
        let stored = StoredRootPreferences {
            version: SETTINGS_VERSION,
            projects: self.projects.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&stored).map_err(SettingsError::Serialize)?;
        let mut temporary = NamedTempFile::new_in(parent).map_err(SettingsError::Io)?;
        temporary.write_all(&bytes).map_err(SettingsError::Io)?;
        temporary.flush().map_err(SettingsError::Io)?;
        temporary.as_file().sync_all().map_err(SettingsError::Io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| SettingsError::Io(error.error))?;
        sync_directory(parent)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredEnginePreferences {
    version: u16,
    projects: HashMap<String, LatexEngine>,
}

pub struct EnginePreferences {
    path: PathBuf,
    projects: HashMap<String, LatexEngine>,
}

impl EnginePreferences {
    pub fn load(path: PathBuf) -> Result<Self, SettingsError> {
        let projects = match fs::read(&path) {
            Ok(bytes) => {
                let stored: StoredEnginePreferences =
                    serde_json::from_slice(&bytes).map_err(SettingsError::Invalid)?;
                if stored.version != SETTINGS_VERSION {
                    return Err(SettingsError::UnsupportedVersion(stored.version));
                }
                for project_id in stored.projects.keys() {
                    ProjectId::parse(project_id).map_err(|_| SettingsError::InvalidProject)?;
                }
                stored.projects
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(SettingsError::Io(error)),
        };
        Ok(Self { path, projects })
    }

    pub fn get(&self, project_id: &str) -> Option<LatexEngine> {
        self.projects.get(project_id).copied()
    }

    pub fn set(
        &mut self,
        project_id: &str,
        engine: Option<LatexEngine>,
    ) -> Result<(), SettingsError> {
        let project_id = ProjectId::parse(project_id).map_err(|_| SettingsError::InvalidProject)?;
        let key = project_id.as_str().to_owned();
        let previous = match engine {
            Some(engine) => self.projects.insert(key.clone(), engine),
            None => self.projects.remove(&key),
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

    fn persist(&self) -> Result<(), SettingsError> {
        let parent = self.path.parent().ok_or(SettingsError::InvalidLocation)?;
        fs::create_dir_all(parent).map_err(SettingsError::Io)?;
        restrict_directory(parent)?;
        let bytes = serde_json::to_vec_pretty(&StoredEnginePreferences {
            version: SETTINGS_VERSION,
            projects: self.projects.clone(),
        })
        .map_err(SettingsError::Serialize)?;
        let mut temporary = NamedTempFile::new_in(parent).map_err(SettingsError::Io)?;
        temporary.write_all(&bytes).map_err(SettingsError::Io)?;
        temporary.flush().map_err(SettingsError::Io)?;
        temporary.as_file().sync_all().map_err(SettingsError::Io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| SettingsError::Io(error.error))?;
        sync_directory(parent)?;
        Ok(())
    }
}

fn restrict_directory(path: &Path) -> Result<(), SettingsError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(SettingsError::Io)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), SettingsError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(SettingsError::Io)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("settings path has no parent directory")]
    InvalidLocation,
    #[error("project identity is invalid")]
    InvalidProject,
    #[error("stored settings are invalid: {0}")]
    Invalid(#[source] serde_json::Error),
    #[error("settings version {0} is unsupported")]
    UnsupportedVersion(u16),
    #[error("root document path is unsafe: {0}")]
    UnsafePath(#[source] ProjectPathError),
    #[error("unable to serialize settings: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("settings filesystem operation failed: {0}")]
    Io(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn preferences_round_trip_outside_the_project() {
        let directory = tempdir().expect("settings directory");
        let path = directory.path().join("root-documents.json");
        let project_id = "a".repeat(64);
        let mut preferences = RootPreferences::load(path.clone()).expect("empty settings");
        preferences
            .set(&project_id, "paper/main.tex")
            .expect("persist preference");

        let loaded = RootPreferences::load(path).expect("reload settings");
        assert_eq!(loaded.get(&project_id), Some("paper/main.tex"));
    }

    #[test]
    fn engine_preferences_are_explicit_persistent_and_clearable() {
        let directory = tempdir().expect("settings directory");
        let path = directory.path().join("engines.json");
        let project_id = "c".repeat(64);
        let mut preferences = EnginePreferences::load(path.clone()).unwrap();
        assert_eq!(preferences.get(&project_id), None);
        preferences
            .set(&project_id, Some(LatexEngine::XeLatex))
            .unwrap();
        assert_eq!(
            EnginePreferences::load(path.clone())
                .unwrap()
                .get(&project_id),
            Some(LatexEngine::XeLatex)
        );
        preferences.set(&project_id, None).unwrap();
        assert_eq!(
            EnginePreferences::load(path).unwrap().get(&project_id),
            None
        );
    }

    #[test]
    fn rejects_traversal_and_corrupt_settings() {
        let directory = tempdir().expect("settings directory");
        let path = directory.path().join("root-documents.json");
        let mut preferences = RootPreferences::load(path.clone()).expect("empty settings");
        assert!(matches!(
            preferences.set(&"b".repeat(64), "../main.tex"),
            Err(SettingsError::UnsafePath(_))
        ));
        fs::write(&path, b"not json").expect("corrupt fixture");
        assert!(matches!(
            RootPreferences::load(path),
            Err(SettingsError::Invalid(_))
        ));
    }
}
