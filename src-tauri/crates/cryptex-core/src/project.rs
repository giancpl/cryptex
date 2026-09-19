use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn from_canonical_root(root: &Path) -> Self {
        let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
        Self(format!("{digest:x}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProjectPath(PathBuf);

impl ProjectPath {
    pub fn parse(path: impl AsRef<Path>) -> Result<Self, ProjectPathError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(ProjectPathError::Empty);
        }
        if path.is_absolute() {
            return Err(ProjectPathError::Absolute);
        }
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(ProjectPathError::Traversal);
        }
        if path.as_os_str().as_encoded_bytes().contains(&0) {
            return Err(ProjectPathError::NulByte);
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct ProjectRoot {
    canonical: PathBuf,
    id: ProjectId,
}

impl ProjectRoot {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ProjectPathError> {
        let canonical = root
            .as_ref()
            .canonicalize()
            .map_err(ProjectPathError::Canonicalize)?;
        if !canonical.is_dir() {
            return Err(ProjectPathError::NotDirectory);
        }
        let id = ProjectId::from_canonical_root(&canonical);
        Ok(Self { canonical, id })
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical
    }

    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    pub fn resolve_existing(&self, path: &ProjectPath) -> Result<PathBuf, ProjectPathError> {
        let resolved = self
            .canonical
            .join(path.as_path())
            .canonicalize()
            .map_err(ProjectPathError::Canonicalize)?;
        if !resolved.starts_with(&self.canonical) {
            return Err(ProjectPathError::OutsideRoot);
        }
        Ok(resolved)
    }
}

#[derive(Debug, Error)]
pub enum ProjectPathError {
    #[error("project path must not be empty")]
    Empty,
    #[error("project path must be relative")]
    Absolute,
    #[error("project path contains traversal or non-normal components")]
    Traversal,
    #[error("project path contains a NUL byte")]
    NulByte,
    #[error("project root is not a directory")]
    NotDirectory,
    #[error("project path resolves outside the project root")]
    OutsideRoot,
    #[error("unable to canonicalize project path: {0}")]
    Canonicalize(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn accepts_only_normal_relative_paths() {
        assert!(ProjectPath::parse("chapters/intro.tex").is_ok());
        assert!(matches!(ProjectPath::parse(""), Err(ProjectPathError::Empty)));
        assert!(matches!(ProjectPath::parse("/tmp/main.tex"), Err(ProjectPathError::Absolute)));
        assert!(matches!(ProjectPath::parse("../main.tex"), Err(ProjectPathError::Traversal)));
        assert!(matches!(ProjectPath::parse("./main.tex"), Err(ProjectPathError::Traversal)));
    }

    #[test]
    fn project_identity_is_stable_for_the_same_canonical_root() {
        let directory = tempdir().expect("temporary project");
        let first = ProjectRoot::open(directory.path()).expect("valid project");
        let second = ProjectRoot::open(directory.path().join(".")).expect("valid project");
        assert_eq!(first.id(), second.id());
    }

    #[test]
    fn resolves_existing_files_inside_root() {
        let directory = tempdir().expect("temporary project");
        fs::write(directory.path().join("main.tex"), "test").expect("fixture file");
        let root = ProjectRoot::open(directory.path()).expect("valid project");
        let path = ProjectPath::parse("main.tex").expect("valid relative path");
        assert_eq!(root.resolve_existing(&path).expect("safe path"), directory.path().join("main.tex"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_that_resolve_outside_root() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().expect("temporary project");
        let outside = tempdir().expect("outside directory");
        fs::write(outside.path().join("secret.tex"), "secret").expect("fixture file");
        symlink(outside.path().join("secret.tex"), directory.path().join("escape.tex"))
            .expect("fixture symlink");
        let root = ProjectRoot::open(directory.path()).expect("valid project");
        let path = ProjectPath::parse("escape.tex").expect("valid relative path");
        assert!(matches!(root.resolve_existing(&path), Err(ProjectPathError::OutsideRoot)));
    }
}
