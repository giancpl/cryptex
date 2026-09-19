use crate::api::{
    API_VERSION, FileTreeEntry, FileTreeEntryKind, FileTreePage, ProjectSummary, TextDocument,
    WriteResult,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use thiserror::Error;
use tempfile::NamedTempFile;

pub const MAX_DIRECTORY_ENTRIES: usize = 5_000;
pub const MAX_TEXT_FILE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn from_canonical_root(root: &Path) -> Self {
        let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
        Self(format!("{digest:x}"))
    }

    pub fn parse(value: &str) -> Result<Self, ProjectError> {
        if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err(ProjectError::UnknownProject)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProjectPath(PathBuf);

impl ProjectPath {
    pub fn root() -> Self {
        Self(PathBuf::new())
    }

    pub fn parse(path: impl AsRef<Path>) -> Result<Self, ProjectPathError> {
        let path = path.as_ref();
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

    fn display(&self) -> String {
        self.0.to_string_lossy().into_owned()
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

#[derive(Default)]
pub struct ProjectService {
    projects: HashMap<ProjectId, ProjectRoot>,
}

impl ProjectService {
    pub fn open(&mut self, root: impl AsRef<Path>) -> Result<ProjectSummary, ProjectError> {
        let project = ProjectRoot::open(root)?;
        let name = project
            .canonical_path()
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|name| !name.is_empty())
            .unwrap_or("Project")
            .to_owned();
        let summary = ProjectSummary {
            api_version: API_VERSION,
            project_id: project.id().as_str().to_owned(),
            name,
            canonical_root: project.canonical_path().to_string_lossy().into_owned(),
        };
        self.projects.insert(project.id().clone(), project);
        Ok(summary)
    }

    pub fn list_directory(
        &self,
        project_id: &str,
        relative_path: &str,
    ) -> Result<FileTreePage, ProjectError> {
        let id = ProjectId::parse(project_id)?;
        let project = self.projects.get(&id).ok_or(ProjectError::UnknownProject)?;
        let path = if relative_path.is_empty() {
            ProjectPath::root()
        } else {
            ProjectPath::parse(relative_path)?
        };
        let directory = project.resolve_existing(&path)?;
        if !directory.is_dir() {
            return Err(ProjectError::NotDirectory);
        }

        let mut entries = fs::read_dir(&directory)
            .map_err(ProjectError::Io)?
            .take(MAX_DIRECTORY_ENTRIES + 1)
            .map(|result| self.map_entry(project, &path, result))
            .collect::<Result<Vec<_>, _>>()?;
        let truncated = entries.len() > MAX_DIRECTORY_ENTRIES;
        entries.truncate(MAX_DIRECTORY_ENTRIES);
        entries.sort_by(|left, right| {
            entry_order(&left.kind)
                .cmp(&entry_order(&right.kind))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.name.cmp(&right.name))
        });

        Ok(FileTreePage {
            api_version: API_VERSION,
            directory: path.display(),
            entries,
            truncated,
        })
    }

    pub fn read_text_file(
        &self,
        project_id: &str,
        relative_path: &str,
    ) -> Result<TextDocument, ProjectError> {
        let (path, resolved) = self.resolve_file(project_id, relative_path)?;
        let bytes = read_bounded(&resolved)?;
        if bytes.contains(&0) {
            return Err(ProjectError::BinaryFile);
        }
        let text = String::from_utf8(bytes).map_err(|_| ProjectError::InvalidUtf8)?;
        let size_bytes = text.len() as u64;
        Ok(TextDocument {
            api_version: API_VERSION,
            relative_path: path.display(),
            fingerprint: fingerprint(text.as_bytes()),
            text,
            size_bytes,
        })
    }

    pub fn write_text_file(
        &self,
        project_id: &str,
        relative_path: &str,
        text: &str,
        expected_fingerprint: &str,
    ) -> Result<WriteResult, ProjectError> {
        if text.len() as u64 > MAX_TEXT_FILE_BYTES {
            return Err(ProjectError::FileTooLarge);
        }
        let (path, resolved) = self.resolve_file(project_id, relative_path)?;
        let original = read_bounded(&resolved)?;
        if fingerprint(&original) != expected_fingerprint {
            return Err(ProjectError::StaleFingerprint);
        }
        let permissions = fs::metadata(&resolved).map_err(ProjectError::Io)?.permissions();
        let parent = resolved.parent().ok_or(ProjectError::InvalidParent)?;
        let mut temporary = NamedTempFile::new_in(parent).map_err(ProjectError::Io)?;
        temporary.write_all(text.as_bytes()).map_err(ProjectError::Io)?;
        temporary.flush().map_err(ProjectError::Io)?;
        temporary.as_file().sync_all().map_err(ProjectError::Io)?;
        temporary.as_file().set_permissions(permissions).map_err(ProjectError::Io)?;

        // Revalidate immediately before replacement. This prevents normal editor races;
        // callers must still treat filesystem writes as fallible external operations.
        if fingerprint(&read_bounded(&resolved)?) != expected_fingerprint {
            return Err(ProjectError::StaleFingerprint);
        }
        temporary.persist(&resolved).map_err(|error| ProjectError::Io(error.error))?;
        sync_directory(parent)?;

        Ok(WriteResult {
            api_version: API_VERSION,
            relative_path: path.display(),
            fingerprint: fingerprint(text.as_bytes()),
            size_bytes: text.len() as u64,
        })
    }

    fn resolve_file(
        &self,
        project_id: &str,
        relative_path: &str,
    ) -> Result<(ProjectPath, PathBuf), ProjectError> {
        let id = ProjectId::parse(project_id)?;
        let project = self.projects.get(&id).ok_or(ProjectError::UnknownProject)?;
        let path = ProjectPath::parse(relative_path)?;
        if path.as_path().as_os_str().is_empty() {
            return Err(ProjectError::NotFile);
        }
        let resolved = project.resolve_existing(&path)?;
        if !resolved.is_file() {
            return Err(ProjectError::NotFile);
        }
        Ok((path, resolved))
    }

    fn map_entry(
        &self,
        project: &ProjectRoot,
        parent: &ProjectPath,
        result: Result<fs::DirEntry, std::io::Error>,
    ) -> Result<FileTreeEntry, ProjectError> {
        let entry = result.map_err(ProjectError::Io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = parent.as_path().join(&name);
        let metadata = fs::symlink_metadata(entry.path()).map_err(ProjectError::Io)?;
        let is_symlink = metadata.file_type().is_symlink();
        let resolved = entry.path().canonicalize();
        let accessible = resolved
            .as_ref()
            .is_ok_and(|path| path.starts_with(project.canonical_path()));
        let kind = if is_symlink && !accessible {
            FileTreeEntryKind::Symlink
        } else if resolved.as_ref().is_ok_and(|path| path.is_dir()) {
            FileTreeEntryKind::Directory
        } else if resolved.as_ref().is_ok_and(|path| path.is_file()) {
            FileTreeEntryKind::File
        } else {
            FileTreeEntryKind::Other
        };

        Ok(FileTreeEntry {
            name: name.clone(),
            relative_path: relative.to_string_lossy().into_owned(),
            kind,
            is_symlink,
            accessible,
            hidden: name.starts_with('.'),
            generated: is_generated_file(Path::new(&name)),
        })
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ProjectError> {
    let file = File::open(path).map_err(ProjectError::Io)?;
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(ProjectError::Io)?;
    if bytes.len() as u64 > MAX_TEXT_FILE_BYTES {
        return Err(ProjectError::FileTooLarge);
    }
    Ok(bytes)
}

fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sync_directory(path: &Path) -> Result<(), ProjectError> {
    #[cfg(unix)]
    File::open(path).and_then(|directory| directory.sync_all()).map_err(ProjectError::Io)?;
    Ok(())
}

fn entry_order(kind: &FileTreeEntryKind) -> u8 {
    match kind {
        FileTreeEntryKind::Directory => 0,
        FileTreeEntryKind::File => 1,
        FileTreeEntryKind::Symlink => 2,
        FileTreeEntryKind::Other => 3,
    }
}

fn is_generated_file(path: &Path) -> bool {
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    name.ends_with(".synctex.gz")
        || name.ends_with(".run.xml")
        || matches!(
            path.extension().and_then(OsStr::to_str),
            Some("aux" | "bbl" | "bcf" | "blg" | "fdb_latexmk" | "fls" | "log" | "out" | "toc")
        )
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Path(#[from] ProjectPathError),
    #[error("project is not open")]
    UnknownProject,
    #[error("requested path is not a directory")]
    NotDirectory,
    #[error("requested path is not a regular file")]
    NotFile,
    #[error("file exceeds the 5 MiB text editing limit")]
    FileTooLarge,
    #[error("file appears to contain binary data")]
    BinaryFile,
    #[error("file is not valid UTF-8")]
    InvalidUtf8,
    #[error("file changed on disk since it was read")]
    StaleFingerprint,
    #[error("file has no valid parent directory")]
    InvalidParent,
    #[error("filesystem operation failed: {0}")]
    Io(#[source] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ProjectPathError {
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
    fn accepts_root_and_normal_relative_paths() {
        assert!(ProjectPath::parse("").is_ok());
        assert!(ProjectPath::parse("chapters/intro.tex").is_ok());
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
    fn lists_directories_before_files_with_stable_names() {
        let directory = tempdir().expect("temporary project");
        fs::create_dir(directory.path().join("chapters")).expect("fixture directory");
        fs::write(directory.path().join("Z.tex"), "test").expect("fixture file");
        fs::write(directory.path().join("a.tex"), "test").expect("fixture file");
        let mut service = ProjectService::default();
        let project = service.open(directory.path()).expect("open project");
        let page = service.list_directory(&project.project_id, "").expect("list root");
        let names = page.entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>();
        assert_eq!(names, vec!["chapters", "a.tex", "Z.tex"]);
    }

    #[test]
    fn marks_hidden_and_generated_files() {
        let directory = tempdir().expect("temporary project");
        fs::write(directory.path().join(".hidden"), "test").expect("fixture file");
        fs::write(directory.path().join("main.aux"), "test").expect("fixture file");
        let mut service = ProjectService::default();
        let project = service.open(directory.path()).expect("open project");
        let page = service.list_directory(&project.project_id, "").expect("list root");
        assert!(page.entries.iter().find(|entry| entry.name == ".hidden").expect("hidden").hidden);
        assert!(page.entries.iter().find(|entry| entry.name == "main.aux").expect("generated").generated);
    }

    #[test]
    fn reads_utf8_text_with_a_content_fingerprint() {
        let directory = tempdir().expect("temporary project");
        fs::write(directory.path().join("main.tex"), "Cifratura: π\n").expect("fixture file");
        let mut service = ProjectService::default();
        let project = service.open(directory.path()).expect("open project");
        let document = service.read_text_file(&project.project_id, "main.tex").expect("read text");
        assert_eq!(document.text, "Cifratura: π\n");
        assert_eq!(document.size_bytes, 14);
        assert_eq!(document.fingerprint.len(), 64);
    }

    #[test]
    fn rejects_binary_invalid_utf8_and_oversized_files() {
        let directory = tempdir().expect("temporary project");
        fs::write(directory.path().join("binary.dat"), [1, 0, 2]).expect("binary fixture");
        fs::write(directory.path().join("invalid.tex"), [0xff, 0xfe]).expect("utf8 fixture");
        fs::write(
            directory.path().join("large.tex"),
            vec![b'x'; MAX_TEXT_FILE_BYTES as usize + 1],
        )
        .expect("large fixture");
        let mut service = ProjectService::default();
        let project = service.open(directory.path()).expect("open project");
        assert!(matches!(
            service.read_text_file(&project.project_id, "binary.dat"),
            Err(ProjectError::BinaryFile)
        ));
        assert!(matches!(
            service.read_text_file(&project.project_id, "invalid.tex"),
            Err(ProjectError::InvalidUtf8)
        ));
        assert!(matches!(
            service.read_text_file(&project.project_id, "large.tex"),
            Err(ProjectError::FileTooLarge)
        ));
    }

    #[test]
    fn stale_fingerprint_never_overwrites_an_external_change() {
        let directory = tempdir().expect("temporary project");
        let file = directory.path().join("main.tex");
        fs::write(&file, "original").expect("fixture file");
        let mut service = ProjectService::default();
        let project = service.open(directory.path()).expect("open project");
        let document = service.read_text_file(&project.project_id, "main.tex").expect("read text");
        fs::write(&file, "external edit").expect("external edit");
        assert!(matches!(
            service.write_text_file(
                &project.project_id,
                "main.tex",
                "CrypTex edit",
                &document.fingerprint,
            ),
            Err(ProjectError::StaleFingerprint)
        ));
        assert_eq!(fs::read_to_string(file).expect("preserved file"), "external edit");
    }

    #[test]
    fn atomic_write_returns_the_new_fingerprint() {
        let directory = tempdir().expect("temporary project");
        let file = directory.path().join("main.tex");
        fs::write(&file, "original").expect("fixture file");
        let mut service = ProjectService::default();
        let project = service.open(directory.path()).expect("open project");
        let document = service.read_text_file(&project.project_id, "main.tex").expect("read text");
        let result = service
            .write_text_file(&project.project_id, "main.tex", "updated", &document.fingerprint)
            .expect("atomic write");
        let reread = service.read_text_file(&project.project_id, "main.tex").expect("reread text");
        assert_eq!(reread.text, "updated");
        assert_eq!(result.fingerprint, reread.fingerprint);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_unix_permissions() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let directory = tempdir().expect("temporary project");
        let file = directory.path().join("main.tex");
        fs::write(&file, "original").expect("fixture file");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).expect("fixture permissions");
        let mut service = ProjectService::default();
        let project = service.open(directory.path()).expect("open project");
        let document = service.read_text_file(&project.project_id, "main.tex").expect("read text");
        service
            .write_text_file(&project.project_id, "main.tex", "updated", &document.fingerprint)
            .expect("atomic write");
        assert_eq!(fs::metadata(file).expect("metadata").mode() & 0o777, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_resolution_and_marks_symlinks_outside_root_inaccessible() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().expect("temporary project");
        let outside = tempdir().expect("outside directory");
        fs::write(outside.path().join("secret.tex"), "secret").expect("fixture file");
        symlink(outside.path().join("secret.tex"), directory.path().join("escape.tex"))
            .expect("fixture symlink");
        let mut service = ProjectService::default();
        let summary = service.open(directory.path()).expect("open project");
        let root = service.projects.get(&ProjectId::parse(&summary.project_id).expect("id")).expect("root");
        let path = ProjectPath::parse("escape.tex").expect("valid relative path");
        assert!(matches!(root.resolve_existing(&path), Err(ProjectPathError::OutsideRoot)));
        let page = service.list_directory(&summary.project_id, "").expect("list root");
        let escape = page.entries.iter().find(|entry| entry.name == "escape.tex").expect("symlink entry");
        assert!(!escape.accessible);
        assert_eq!(escape.kind, FileTreeEntryKind::Symlink);
    }
}
