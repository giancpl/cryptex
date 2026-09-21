use crate::api::{API_VERSION, ProjectFileChange, ProjectFileChangeKind};
use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

const DEBOUNCE: Duration = Duration::from_millis(100);
const SELF_WRITE_TTL: Duration = Duration::from_secs(5);

type ExpectedWrites = Arc<Mutex<HashMap<PathBuf, (String, Instant)>>>;

pub struct ProjectWatcher {
    _watcher: RecommendedWatcher,
    root: PathBuf,
    expected_writes: ExpectedWrites,
}

impl ProjectWatcher {
    pub fn start(
        project_id: String,
        root: PathBuf,
        emit: impl Fn(ProjectFileChange) + Send + 'static,
    ) -> Result<Self, WatchError> {
        let root = root.canonicalize().map_err(WatchError::Io)?;
        let (sender, receiver) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |event| {
                let _ = sender.send(event);
            },
            Config::default(),
        )
        .map_err(WatchError::Notify)?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(WatchError::Notify)?;

        let expected_writes = ExpectedWrites::default();
        let worker_root = root.clone();
        let worker_expected = Arc::clone(&expected_writes);
        thread::Builder::new()
            .name(format!(
                "cryptex-watch-{}",
                &project_id[..project_id.len().min(8)]
            ))
            .spawn(move || {
                while let Ok(first) = receiver.recv() {
                    let mut events = vec![first];
                    while let Ok(event) = receiver.recv_timeout(DEBOUNCE) {
                        events.push(event);
                    }
                    for change in
                        normalize_batch(&project_id, &worker_root, &worker_expected, events)
                    {
                        emit(change);
                    }
                }
            })
            .map_err(WatchError::Thread)?;

        Ok(Self {
            _watcher: watcher,
            root,
            expected_writes,
        })
    }

    pub fn expect_write(&self, relative_path: &str, fingerprint: String) {
        if let Some(path) = safe_relative_path(relative_path)
            && let Ok(mut expected) = self.expected_writes.lock()
        {
            expected.insert(self.root.join(path), (fingerprint, Instant::now()));
        }
    }

    pub fn expect_text_write(&self, relative_path: &str, text: &str) {
        self.expect_write(
            relative_path,
            format!("{:x}", Sha256::digest(text.as_bytes())),
        );
    }
}

fn normalize_batch(
    project_id: &str,
    root: &Path,
    expected_writes: &ExpectedWrites,
    events: Vec<Result<Event, notify::Error>>,
) -> Vec<ProjectFileChange> {
    let mut grouped: BTreeMap<ProjectFileChangeKind, Vec<PathBuf>> = BTreeMap::new();
    let mut rescan = false;
    for event in events {
        match event {
            Ok(event) => {
                let kind = change_kind(&event.kind);
                let paths = grouped.entry(kind).or_default();
                for path in event.paths {
                    if path.starts_with(root) && !paths.contains(&path) {
                        paths.push(path);
                    }
                }
            }
            Err(_) => rescan = true,
        }
    }

    let now = Instant::now();
    let mut expected = expected_writes.lock().ok();
    if let Some(expected) = expected.as_mut() {
        expected.retain(|_, (_, recorded)| now.duration_since(*recorded) <= SELF_WRITE_TTL);
    }

    let mut changes = grouped
        .into_iter()
        .filter_map(|(kind, absolute_paths)| {
            let relative_paths = absolute_paths
                .iter()
                .filter_map(|path| path.strip_prefix(root).ok())
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            if relative_paths.is_empty() {
                return None;
            }
            let self_write = expected.as_mut().is_some_and(|expected| {
                absolute_paths.iter().any(|path| {
                    let matches = expected.get(path).is_some_and(|(fingerprint, _)| {
                        file_fingerprint(path).as_ref() == Some(fingerprint)
                    });
                    if matches {
                        expected.remove(path);
                    }
                    matches
                })
            });
            Some(ProjectFileChange {
                api_version: API_VERSION,
                project_id: project_id.to_owned(),
                relative_paths,
                kind,
                self_write,
            })
        })
        .collect::<Vec<_>>();
    if rescan {
        changes.push(ProjectFileChange {
            api_version: API_VERSION,
            project_id: project_id.to_owned(),
            relative_paths: Vec::new(),
            kind: ProjectFileChangeKind::Rescan,
            self_write: false,
        });
    }
    changes
}

fn change_kind(kind: &EventKind) -> ProjectFileChangeKind {
    match kind {
        EventKind::Create(_) => ProjectFileChangeKind::Create,
        EventKind::Remove(_) => ProjectFileChangeKind::Remove,
        EventKind::Modify(ModifyKind::Name(_)) => ProjectFileChangeKind::Rename,
        _ => ProjectFileChangeKind::Modify,
    }
}

fn safe_relative_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    (!path.is_absolute()
        && !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_))))
    .then(|| path.to_path_buf())
}

fn file_fingerprint(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("unable to access project root: {0}")]
    Io(#[source] std::io::Error),
    #[error("filesystem watcher failed: {0}")]
    Notify(#[source] notify::Error),
    #[error("unable to start watcher worker: {0}")]
    Thread(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use tempfile::tempdir;

    #[test]
    fn recursive_watcher_reports_external_create() {
        let directory = tempdir().expect("temporary project");
        fs::create_dir(directory.path().join("chapters")).expect("fixture directory");
        let (sender, receiver) = mpsc::channel();
        let _watcher = ProjectWatcher::start(
            "a".repeat(64),
            directory.path().to_path_buf(),
            move |change| sender.send(change).expect("send change"),
        )
        .expect("start watcher");

        fs::write(directory.path().join("chapters/intro.tex"), "intro").expect("external create");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut found = false;
        while Instant::now() < deadline {
            if let Ok(change) = receiver.recv_timeout(Duration::from_millis(250)) {
                found |= change
                    .relative_paths
                    .iter()
                    .any(|path| path == "chapters/intro.tex");
                if found {
                    break;
                }
            }
        }
        assert!(found, "recursive file event was not reported");
    }

    #[test]
    fn correlates_expected_write_by_final_fingerprint() {
        let directory = tempdir().expect("temporary project");
        let file = directory.path().join("main.tex");
        fs::write(&file, "old").expect("fixture file");
        let (sender, receiver) = mpsc::channel();
        let watcher = ProjectWatcher::start(
            "b".repeat(64),
            directory.path().to_path_buf(),
            move |change| sender.send(change).expect("send change"),
        )
        .expect("start watcher");
        watcher.expect_write("main.tex", format!("{:x}", Sha256::digest(b"new")));
        fs::write(&file, "new").expect("expected write");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut correlated = false;
        while Instant::now() < deadline {
            if let Ok(change) = receiver.recv_timeout(Duration::from_millis(250)) {
                correlated |= change.self_write
                    && change.relative_paths.iter().any(|path| path == "main.tex");
                if correlated {
                    break;
                }
            }
        }
        assert!(correlated, "expected write was not correlated");
    }
}
