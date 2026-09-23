use super::{
    FileIndexStatus, IndexCompleteness, IndexIssue, IndexIssueCode, IndexRecordKind, IndexedFile,
    PROJECT_INDEX_SCHEMA_VERSION, ProjectIndex, ScannerLimits,
    scanner::{ScanError, scan_latex_file},
};
use crate::api::{API_VERSION, ProjectFileChange, ProjectFileChangeKind};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexUpdate {
    pub generation: u64,
    pub scanned_files: Vec<String>,
    pub removed_files: Vec<String>,
}

#[derive(Clone, Default)]
pub struct IndexCancellationToken(Arc<AtomicBool>);

impl IndexCancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub struct ProjectIndexer {
    project_id: String,
    root: PathBuf,
    limits: ScannerLimits,
    generation: u64,
    files: BTreeMap<String, IndexedFile>,
    edges: BTreeMap<String, BTreeSet<String>>,
    reverse_edges: BTreeMap<String, BTreeSet<String>>,
    issues: Vec<IndexIssue>,
}

impl ProjectIndexer {
    pub fn open(
        project_id: String,
        root: impl AsRef<Path>,
        limits: ScannerLimits,
    ) -> Result<Self, ProjectIndexError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(ProjectIndexError::Io)?;
        if !root.is_dir() {
            return Err(ProjectIndexError::InvalidRoot);
        }
        let mut indexer = Self {
            project_id,
            root,
            limits,
            generation: 0,
            files: BTreeMap::new(),
            edges: BTreeMap::new(),
            reverse_edges: BTreeMap::new(),
            issues: Vec::new(),
        };
        indexer.rescan()?;
        Ok(indexer)
    }

    pub fn rescan(&mut self) -> Result<IndexUpdate, ProjectIndexError> {
        self.rescan_cancellable(&IndexCancellationToken::default())
    }

    pub fn rescan_cancellable(
        &mut self,
        cancellation: &IndexCancellationToken,
    ) -> Result<IndexUpdate, ProjectIndexError> {
        let mut paths = Vec::new();
        let mut visited = HashSet::new();
        collect_sources(
            &self.root,
            &self.root,
            &mut paths,
            self.limits.max_project_files as usize + 1,
            cancellation,
            &mut visited,
        )?;
        paths.sort();
        let file_limit_reached = paths.len() > self.limits.max_project_files as usize;
        paths.truncate(self.limits.max_project_files as usize);
        let mut total = 0_u64;
        let mut next = BTreeMap::new();
        let mut scanned = Vec::new();
        let mut project_issues = Vec::new();
        if file_limit_reached {
            project_issues.push(project_issue(
                IndexIssueCode::RecordLimitReached,
                "project file count exceeds the configured indexing limit",
            ));
        }
        for path in paths {
            if cancellation.is_cancelled() {
                return Err(ProjectIndexError::Cancelled);
            }
            let relative = relative_string(&self.root, &path)?;
            let size = fs::metadata(&path).map_err(ProjectIndexError::Io)?.len();
            if total.saturating_add(size) > self.limits.max_total_bytes {
                project_issues.push(project_issue(
                    IndexIssueCode::FileTooLarge,
                    "project source bytes exceed the configured indexing limit",
                ));
                break;
            }
            total += size;
            let file = scan_path(&self.root, &relative, self.limits)?;
            scanned.push(relative.clone());
            next.insert(relative, file);
        }
        if cancellation.is_cancelled() {
            return Err(ProjectIndexError::Cancelled);
        }
        let removed = self
            .files
            .keys()
            .filter(|path| !next.contains_key(*path))
            .cloned()
            .collect();
        self.files = next;
        self.issues = project_issues;
        self.generation = self.generation.saturating_add(1);
        self.rebuild_graph();
        Ok(IndexUpdate {
            generation: self.generation,
            scanned_files: scanned,
            removed_files: removed,
        })
    }

    pub fn apply_change(
        &mut self,
        change: &ProjectFileChange,
    ) -> Result<IndexUpdate, ProjectIndexError> {
        if change.project_id != self.project_id {
            return Err(ProjectIndexError::WrongProject);
        }
        if change.kind == ProjectFileChangeKind::Rescan {
            return self.rescan();
        }
        let mut scanned = Vec::new();
        let mut removed = Vec::new();
        for relative in &change.relative_paths {
            if !is_source(relative) {
                continue;
            }
            let path = safe_source_path(&self.root, relative);
            match path {
                Ok(path) if path.is_file() => {
                    let file = scan_path(&self.root, relative, self.limits)?;
                    self.files.insert(relative.clone(), file);
                    scanned.push(relative.clone());
                }
                _ => {
                    if self.files.remove(relative).is_some() {
                        removed.push(relative.clone());
                    }
                }
            }
        }
        if !scanned.is_empty() || !removed.is_empty() {
            self.generation = self.generation.saturating_add(1);
            self.rebuild_graph();
        }
        Ok(IndexUpdate {
            generation: self.generation,
            scanned_files: scanned,
            removed_files: removed,
        })
    }

    pub fn snapshot(&self) -> ProjectIndex {
        let mut files: Vec<_> = self.files.values().cloned().collect();
        for file in &mut files {
            for record in &mut file.records {
                if record.kind == IndexRecordKind::Include {
                    record.target = resolve_include(&file.relative_path, &record.name)
                        .filter(|target| self.files.contains_key(target));
                }
            }
        }
        ProjectIndex {
            api_version: API_VERSION,
            schema_version: PROJECT_INDEX_SCHEMA_VERSION,
            project_id: self.project_id.clone(),
            generation: self.generation,
            completeness: IndexCompleteness::BestEffort,
            scanner_limits: self.limits,
            files,
            issues: self.issues.clone(),
        }
    }

    pub fn dependencies(&self, relative_path: &str) -> Vec<String> {
        self.edges
            .get(relative_path)
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    pub fn reverse_dependencies(&self, relative_path: &str) -> Vec<String> {
        self.reverse_edges
            .get(relative_path)
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    fn rebuild_graph(&mut self) {
        self.edges.clear();
        self.reverse_edges.clear();
        self.issues.retain(|issue| issue.relative_path.is_none());
        for (source, file) in &self.files {
            for record in &file.records {
                if record.kind != IndexRecordKind::Include {
                    continue;
                }
                let Some(target) = resolve_include(source, &record.name) else {
                    self.issues.push(IndexIssue {
                        code: IndexIssueCode::MissingInclude,
                        message: format!(
                            "include target is not a safe project-relative path: {}",
                            record.name
                        ),
                        relative_path: Some(source.clone()),
                        range: Some(record.range),
                    });
                    continue;
                };
                if !self.files.contains_key(&target) {
                    self.issues.push(IndexIssue {
                        code: IndexIssueCode::MissingInclude,
                        message: format!("included file was not indexed: {target}"),
                        relative_path: Some(source.clone()),
                        range: Some(record.range),
                    });
                    continue;
                }
                self.edges
                    .entry(source.clone())
                    .or_default()
                    .insert(target.clone());
                self.reverse_edges
                    .entry(target)
                    .or_default()
                    .insert(source.clone());
            }
        }
        let cycles = find_cycle_nodes(&self.edges);
        for path in cycles {
            self.issues.push(IndexIssue {
                code: IndexIssueCode::IncludeCycle,
                message: "file participates in an include cycle".to_owned(),
                relative_path: Some(path),
                range: None,
            });
        }
        self.issues.sort_by(|left, right| {
            (&left.relative_path, &left.message).cmp(&(&right.relative_path, &right.message))
        });
    }
}

fn scan_path(
    root: &Path,
    relative: &str,
    limits: ScannerLimits,
) -> Result<IndexedFile, ProjectIndexError> {
    let path = safe_source_path(root, relative)?;
    let bytes = fs::read(&path).map_err(ProjectIndexError::Io)?;
    let fingerprint = format!("{:x}", Sha256::digest(&bytes));
    let source = match std::str::from_utf8(&bytes) {
        Ok(source) => source,
        Err(_) => {
            return Ok(IndexedFile {
                relative_path: relative.to_owned(),
                fingerprint,
                status: FileIndexStatus::Skipped,
                records: Vec::new(),
                issues: vec![IndexIssue {
                    code: IndexIssueCode::UnsupportedEncoding,
                    message: "file is not valid UTF-8".to_owned(),
                    relative_path: Some(relative.to_owned()),
                    range: None,
                }],
            });
        }
    };
    scan_latex_file(relative, &fingerprint, source, limits).map_err(ProjectIndexError::Scan)
}

fn collect_sources(
    root: &Path,
    directory: &Path,
    output: &mut Vec<PathBuf>,
    limit: usize,
    cancellation: &IndexCancellationToken,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), ProjectIndexError> {
    if cancellation.is_cancelled() {
        return Err(ProjectIndexError::Cancelled);
    }
    if output.len() >= limit {
        return Ok(());
    }
    let canonical_directory = directory.canonicalize().map_err(ProjectIndexError::Io)?;
    if !canonical_directory.starts_with(root) || !visited.insert(canonical_directory) {
        return Ok(());
    }
    let mut entries = fs::read_dir(directory)
        .map_err(ProjectIndexError::Io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ProjectIndexError::Io)?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if output.len() >= limit {
            break;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(ProjectIndexError::Io)?;
        if metadata.file_type().is_symlink() {
            let Ok(resolved) = path.canonicalize() else {
                continue;
            };
            if !resolved.starts_with(root) {
                continue;
            }
        }
        if path.is_dir() {
            collect_sources(root, &path, output, limit, cancellation, visited)?;
        } else if path.is_file()
            && path
                .strip_prefix(root)
                .ok()
                .and_then(Path::to_str)
                .is_some_and(is_source)
        {
            output.push(path);
        }
    }
    Ok(())
}

fn safe_source_path(root: &Path, relative: &str) -> Result<PathBuf, ProjectIndexError> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ProjectIndexError::UnsafePath);
    }
    let path = root.join(relative_path);
    let resolved = path.canonicalize().map_err(ProjectIndexError::Io)?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err(ProjectIndexError::UnsafePath);
    }
    Ok(resolved)
}

fn relative_string(root: &Path, path: &Path) -> Result<String, ProjectIndexError> {
    path.canonicalize()
        .map_err(ProjectIndexError::Io)?
        .strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .ok_or(ProjectIndexError::UnsafePath)
}

fn is_source(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "tex" | "sty"))
}

fn resolve_include(source: &str, target: &str) -> Option<String> {
    let parent = Path::new(source).parent().unwrap_or_else(|| Path::new(""));
    let mut resolved = PathBuf::new();
    for component in parent.join(target.trim()).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir if resolved.pop() => {}
            _ => return None,
        }
    }
    if resolved.extension().is_none() {
        resolved.set_extension("tex");
    }
    resolved.to_str().map(str::to_owned)
}

fn find_cycle_nodes(edges: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    fn visit(
        node: &str,
        edges: &BTreeMap<String, BTreeSet<String>>,
        active: &mut Vec<String>,
        done: &mut HashSet<String>,
        cycles: &mut BTreeSet<String>,
    ) {
        if let Some(position) = active.iter().position(|item| item == node) {
            cycles.extend(active[position..].iter().cloned());
            return;
        }
        if !done.insert(node.to_owned()) {
            return;
        }
        active.push(node.to_owned());
        for target in edges.get(node).into_iter().flatten() {
            visit(target, edges, active, done, cycles);
        }
        active.pop();
    }
    let mut done = HashSet::new();
    let mut cycles = BTreeSet::new();
    for node in edges.keys() {
        visit(node, edges, &mut Vec::new(), &mut done, &mut cycles);
    }
    cycles
}

fn project_issue(code: IndexIssueCode, message: &str) -> IndexIssue {
    IndexIssue {
        code,
        message: message.to_owned(),
        relative_path: None,
        range: None,
    }
}

#[derive(Debug, Error)]
pub enum ProjectIndexError {
    #[error("unable to access project source: {0}")]
    Io(#[source] std::io::Error),
    #[error("project index root is not a directory")]
    InvalidRoot,
    #[error("change belongs to another project")]
    WrongProject,
    #[error("project index path escaped the root")]
    UnsafePath,
    #[error("project indexing was cancelled")]
    Cancelled,
    #[error("per-file scan failed: {0}")]
    Scan(#[source] ScanError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(root: &Path, relative: &str, source: &[u8]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
    }

    fn change(project_id: &str, kind: ProjectFileChangeKind, paths: &[&str]) -> ProjectFileChange {
        ProjectFileChange {
            api_version: API_VERSION,
            project_id: project_id.to_owned(),
            relative_paths: paths.iter().map(|path| (*path).to_owned()).collect(),
            kind,
            self_write: false,
        }
    }

    #[test]
    fn builds_graph_reverse_dependencies_and_reports_missing_and_cycles() {
        let root = tempdir().unwrap();
        write(
            root.path(),
            "main.tex",
            b"\\input{chapters/a}\\input{missing}",
        );
        write(root.path(), "chapters/a.tex", b"\\input{../main}\\label{a}");
        let indexer =
            ProjectIndexer::open("project".to_owned(), root.path(), ScannerLimits::default())
                .unwrap();

        assert_eq!(indexer.dependencies("main.tex"), vec!["chapters/a.tex"]);
        assert_eq!(
            indexer.reverse_dependencies("chapters/a.tex"),
            vec!["main.tex"]
        );
        let index = indexer.snapshot();
        assert!(
            index
                .issues
                .iter()
                .any(|issue| issue.code == IndexIssueCode::MissingInclude)
        );
        let cycle_files: BTreeSet<_> = index
            .issues
            .iter()
            .filter(|issue| issue.code == IndexIssueCode::IncludeCycle)
            .filter_map(|issue| issue.relative_path.as_deref())
            .collect();
        assert_eq!(cycle_files, BTreeSet::from(["chapters/a.tex", "main.tex"]));
    }

    #[test]
    fn modify_rename_and_delete_only_rescan_changed_existing_files() {
        let root = tempdir().unwrap();
        write(root.path(), "main.tex", b"\\input{one}\\input{two}");
        write(root.path(), "one.tex", b"\\label{one}");
        write(root.path(), "two.tex", b"\\label{two}");
        let mut indexer =
            ProjectIndexer::open("project".to_owned(), root.path(), ScannerLimits::default())
                .unwrap();
        let initial_generation = indexer.snapshot().generation;

        write(root.path(), "one.tex", b"\\label{changed}");
        let update = indexer
            .apply_change(&change(
                "project",
                ProjectFileChangeKind::Modify,
                &["one.tex"],
            ))
            .unwrap();
        assert_eq!(update.scanned_files, vec!["one.tex"]);
        assert!(update.removed_files.is_empty());
        assert_eq!(update.generation, initial_generation + 1);
        assert!(indexer.snapshot().files.iter().any(|file| {
            file.relative_path == "one.tex"
                && file.records.iter().any(|record| record.name == "changed")
        }));

        fs::rename(root.path().join("two.tex"), root.path().join("renamed.tex")).unwrap();
        let update = indexer
            .apply_change(&change(
                "project",
                ProjectFileChangeKind::Rename,
                &["two.tex", "renamed.tex"],
            ))
            .unwrap();
        assert_eq!(update.scanned_files, vec!["renamed.tex"]);
        assert_eq!(update.removed_files, vec!["two.tex"]);
        assert!(
            indexer
                .snapshot()
                .issues
                .iter()
                .any(|issue| issue.code == IndexIssueCode::MissingInclude)
        );

        fs::remove_file(root.path().join("one.tex")).unwrap();
        let update = indexer
            .apply_change(&change(
                "project",
                ProjectFileChangeKind::Remove,
                &["one.tex"],
            ))
            .unwrap();
        assert!(update.scanned_files.is_empty());
        assert_eq!(update.removed_files, vec!["one.tex"]);
    }

    #[test]
    fn cancelled_rescan_does_not_publish_partial_state() {
        let root = tempdir().unwrap();
        write(root.path(), "main.tex", b"\\label{before}");
        let mut indexer =
            ProjectIndexer::open("project".to_owned(), root.path(), ScannerLimits::default())
                .unwrap();
        let before = indexer.snapshot();
        write(root.path(), "new.tex", b"\\label{new}");
        let cancellation = IndexCancellationToken::default();
        cancellation.cancel();
        assert!(matches!(
            indexer.rescan_cancellable(&cancellation),
            Err(ProjectIndexError::Cancelled)
        ));
        assert_eq!(indexer.snapshot(), before);
    }

    #[test]
    fn ignores_non_sources_and_rejects_cross_project_changes() {
        let root = tempdir().unwrap();
        write(root.path(), "main.tex", b"text");
        let mut indexer =
            ProjectIndexer::open("project".to_owned(), root.path(), ScannerLimits::default())
                .unwrap();
        write(root.path(), "image.png", b"image");
        let generation = indexer.snapshot().generation;
        let update = indexer
            .apply_change(&change(
                "project",
                ProjectFileChangeKind::Create,
                &["image.png"],
            ))
            .unwrap();
        assert_eq!(update.generation, generation);
        assert!(matches!(
            indexer.apply_change(&change(
                "other",
                ProjectFileChangeKind::Modify,
                &["main.tex"]
            )),
            Err(ProjectIndexError::WrongProject)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn full_scan_terminates_on_in_root_directory_symlink_cycles() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        write(root.path(), "chapters/a.tex", b"text");
        symlink(root.path(), root.path().join("chapters/back")).unwrap();
        let indexer =
            ProjectIndexer::open("project".to_owned(), root.path(), ScannerLimits::default())
                .unwrap();
        assert_eq!(indexer.snapshot().files.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn full_scan_skips_symlinks_that_escape_the_project() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        write(root.path(), "main.tex", b"text");
        write(outside.path(), "secret.tex", b"secret");
        symlink(
            outside.path().join("secret.tex"),
            root.path().join("escape.tex"),
        )
        .unwrap();
        let indexer =
            ProjectIndexer::open("project".to_owned(), root.path(), ScannerLimits::default())
                .unwrap();
        assert_eq!(indexer.snapshot().files.len(), 1);
        assert_eq!(indexer.snapshot().files[0].relative_path, "main.tex");
    }

    #[test]
    fn non_utf8_files_are_retained_as_skipped_evidence() {
        let root = tempdir().unwrap();
        write(root.path(), "binary.tex", &[0xff, 0xfe]);
        let indexer =
            ProjectIndexer::open("project".to_owned(), root.path(), ScannerLimits::default())
                .unwrap();
        let file = &indexer.snapshot().files[0];
        assert_eq!(file.status, FileIndexStatus::Skipped);
        assert_eq!(file.issues[0].code, IndexIssueCode::UnsupportedEncoding);
    }
}
