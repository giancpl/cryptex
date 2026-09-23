use crate::{
    api::{BuildConfiguration, BuildPermission, LatexEngine},
    project::{ProjectError, ProjectId, ProjectService},
    trust::{TrustError, TrustService},
};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const LATEXMK_EXECUTABLE: &str = "latexmk";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildArtifacts {
    pub directory: PathBuf,
    pub pdf: PathBuf,
    pub synctex: PathBuf,
    pub log: PathBuf,
    pub recorder: PathBuf,
    pub latexmk_database: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LatexmkRequest {
    pub executable_name: String,
    pub arguments: Vec<OsString>,
    pub working_directory: PathBuf,
    pub configuration: BuildConfiguration,
    pub project_rc_enabled: bool,
    pub shell_escape_enabled: bool,
    pub artifacts: BuildArtifacts,
}

pub struct LatexmkRequestBuilder {
    build_cache_root: PathBuf,
}

impl LatexmkRequestBuilder {
    pub fn new(build_cache_root: PathBuf) -> Result<Self, BuildRequestError> {
        fs::create_dir_all(&build_cache_root).map_err(BuildRequestError::Io)?;
        let build_cache_root = build_cache_root
            .canonicalize()
            .map_err(BuildRequestError::Io)?;
        if !build_cache_root.is_dir() {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        Ok(Self { build_cache_root })
    }

    pub fn validate_artifact_file(&self, path: &Path) -> Result<PathBuf, BuildRequestError> {
        let metadata = fs::symlink_metadata(path).map_err(BuildRequestError::Io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        let resolved = path.canonicalize().map_err(BuildRequestError::Io)?;
        if !resolved.starts_with(&self.build_cache_root) || resolved != path {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        Ok(resolved)
    }

    pub fn read_bounded_artifact(
        &self,
        path: &Path,
        max_bytes: u64,
    ) -> Result<Vec<u8>, BuildRequestError> {
        let path = self.validate_artifact_file(path)?;
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(BuildRequestError::Io)?
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(BuildRequestError::Io)?;
        if bytes.len() as u64 > max_bytes {
            return Err(BuildRequestError::ArtifactTooLarge { max_bytes });
        }
        Ok(bytes)
    }

    pub fn retain_pdf_artifact(
        &self,
        source: &Path,
        project_id: &str,
        operation_id: &str,
        max_bytes: u64,
    ) -> Result<PathBuf, BuildRequestError> {
        self.retain_artifact(source, project_id, operation_id, "pdf", max_bytes, true)
    }

    pub fn retain_synctex_artifact(
        &self,
        source: &Path,
        project_id: &str,
        operation_id: &str,
        max_bytes: u64,
    ) -> Result<PathBuf, BuildRequestError> {
        self.retain_artifact(
            source,
            project_id,
            operation_id,
            "synctex.gz",
            max_bytes,
            false,
        )
    }

    fn retain_artifact(
        &self,
        source: &Path,
        project_id: &str,
        operation_id: &str,
        suffix: &str,
        max_bytes: u64,
        require_pdf_markers: bool,
    ) -> Result<PathBuf, BuildRequestError> {
        let project_id = ProjectId::parse(project_id)?;
        let source = self.validate_artifact_file(source)?;
        let project_directory = self.build_cache_root.join(project_id.as_str());
        let project_directory = project_directory
            .canonicalize()
            .map_err(BuildRequestError::Io)?;
        if !project_directory.starts_with(&self.build_cache_root)
            || !project_directory.is_dir()
            || !source.starts_with(&project_directory)
        {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        let bytes = self.read_bounded_artifact(&source, max_bytes)?;
        if require_pdf_markers {
            let header_limit = bytes.len().min(1024);
            let trailer_start = bytes.len().saturating_sub(1024);
            if !bytes[..header_limit]
                .windows(5)
                .any(|value| value == b"%PDF-")
                || !bytes[trailer_start..]
                    .windows(5)
                    .any(|value| value == b"%%EOF")
            {
                return Err(BuildRequestError::InvalidPdfArtifact);
            }
        }
        if operation_id.len() != 26
            || !operation_id.starts_with("build-")
            || !operation_id[6..].bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(BuildRequestError::InvalidOperationId);
        }
        let retained_directory = project_directory.join("published");
        fs::create_dir_all(&retained_directory).map_err(BuildRequestError::Io)?;
        let retained_directory = retained_directory
            .canonicalize()
            .map_err(BuildRequestError::Io)?;
        if !retained_directory.starts_with(&project_directory) || !retained_directory.is_dir() {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        let target = retained_directory.join(format!("{operation_id}.{suffix}"));
        let temporary = retained_directory.join(format!(".{operation_id}.{suffix}.tmp"));
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(BuildRequestError::Io)?;
        let result = (|| {
            output.write_all(&bytes).map_err(BuildRequestError::Io)?;
            output.sync_all().map_err(BuildRequestError::Io)?;
            drop(output);
            fs::rename(&temporary, &target).map_err(BuildRequestError::Io)?;
            self.validate_artifact_file(&target)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn clean(&self, project_id: &str) -> Result<(), BuildRequestError> {
        let project_id = ProjectId::parse(project_id)?;
        let target = self.build_cache_root.join(project_id.as_str());
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(BuildRequestError::Io(error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        let resolved = target.canonicalize().map_err(BuildRequestError::Io)?;
        if !resolved.starts_with(&self.build_cache_root) || resolved == self.build_cache_root {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        fs::remove_dir_all(resolved).map_err(BuildRequestError::Io)
    }

    pub fn build(
        &self,
        projects: &ProjectService,
        trust: &TrustService,
        configuration: BuildConfiguration,
    ) -> Result<LatexmkRequest, BuildRequestError> {
        projects.require_open(&configuration.project_id)?;
        validate_tex_filename(&configuration.root_document)?;
        projects.read_text_file(&configuration.project_id, &configuration.root_document)?;
        let working_directory = projects.project_root(&configuration.project_id)?;
        let artifacts = self.artifacts(&configuration)?;
        let project_rc_enabled = trust
            .allows(&configuration.project_id, BuildPermission::LatexmkRc)?
            && validated_project_rc(&working_directory)?;
        let shell_escape_enabled =
            trust.allows(&configuration.project_id, BuildPermission::ShellEscape)?;

        let mut arguments = vec![OsString::from("-norc")];
        if project_rc_enabled {
            arguments.push(OsString::from("-r"));
            arguments.push(OsString::from(".latexmkrc"));
        }
        arguments.extend([
            OsString::from(engine_argument(configuration.engine)),
            OsString::from("-interaction=nonstopmode"),
            OsString::from("-halt-on-error"),
            OsString::from("-file-line-error"),
            OsString::from("-synctex=1"),
            OsString::from("-recorder"),
            OsString::from("-bibtex"),
            OsString::from(if shell_escape_enabled {
                "-shell-escape"
            } else {
                "-no-shell-escape"
            }),
        ]);
        let mut outdir = OsString::from("-outdir=");
        outdir.push(&artifacts.directory);
        arguments.push(outdir);
        arguments.push(OsString::from(format!("./{}", configuration.root_document)));

        Ok(LatexmkRequest {
            executable_name: LATEXMK_EXECUTABLE.to_owned(),
            arguments,
            working_directory,
            configuration,
            project_rc_enabled,
            shell_escape_enabled,
            artifacts,
        })
    }

    fn artifacts(
        &self,
        configuration: &BuildConfiguration,
    ) -> Result<BuildArtifacts, BuildRequestError> {
        let directory = self.build_cache_root.join(&configuration.project_id);
        fs::create_dir_all(&directory).map_err(BuildRequestError::Io)?;
        let directory = directory.canonicalize().map_err(BuildRequestError::Io)?;
        if !directory.starts_with(&self.build_cache_root) {
            return Err(BuildRequestError::InvalidBuildCache);
        }
        let stem = Path::new(&configuration.root_document)
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or(BuildRequestError::InvalidRootFilename)?;
        Ok(BuildArtifacts {
            pdf: directory.join(format!("{stem}.pdf")),
            synctex: directory.join(format!("{stem}.synctex.gz")),
            log: directory.join(format!("{stem}.log")),
            recorder: directory.join(format!("{stem}.fls")),
            latexmk_database: directory.join(format!("{stem}.fdb_latexmk")),
            directory,
        })
    }
}

fn engine_argument(engine: LatexEngine) -> &'static str {
    match engine {
        LatexEngine::PdfLatex => "-pdf",
        LatexEngine::XeLatex => "-xelatex",
        LatexEngine::LuaLatex => "-lualatex",
    }
}

fn validated_project_rc(project_root: &Path) -> Result<bool, BuildRequestError> {
    let path = project_root.join(".latexmkrc");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(BuildRequestError::Io(error)),
    };
    if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        return Err(BuildRequestError::UnsafeProjectRc);
    }
    let resolved = path.canonicalize().map_err(BuildRequestError::Io)?;
    if !resolved.starts_with(project_root) || !resolved.is_file() {
        return Err(BuildRequestError::UnsafeProjectRc);
    }
    Ok(true)
}

fn validate_tex_filename(value: &str) -> Result<(), BuildRequestError> {
    let path = Path::new(value);
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(BuildRequestError::InvalidRootFilename)?;
    let forbidden = [
        '$', '%', '\\', '~', '"', '\0', '\t', '\u{000c}', '\r', '\n', '\u{007f}',
    ];
    if filename.starts_with('&')
        || value
            .chars()
            .any(|character| forbidden.contains(&character))
        || path.extension().and_then(|extension| extension.to_str()) != Some("tex")
    {
        return Err(BuildRequestError::InvalidRootFilename);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum BuildRequestError {
    #[error("build cache root is invalid")]
    InvalidBuildCache,
    #[error("root document filename is not accepted by latexmk")]
    InvalidRootFilename,
    #[error("build operation identity is invalid")]
    InvalidOperationId,
    #[error("build output is not a complete PDF artifact")]
    InvalidPdfArtifact,
    #[error("project .latexmkrc is not a regular in-project file")]
    UnsafeProjectRc,
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Trust(#[from] TrustError),
    #[error("build artifact exceeds the {max_bytes}-byte limit")]
    ArtifactTooLarge { max_bytes: u64 },
    #[error("build request filesystem operation failed: {0}")]
    Io(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::{API_VERSION, ResolutionProvenance},
        project::ProjectService,
        trust::TrustService,
    };
    use std::fs;
    use tempfile::tempdir;

    struct Fixture {
        _project: tempfile::TempDir,
        _state: tempfile::TempDir,
        projects: ProjectService,
        trust: TrustService,
        builder: LatexmkRequestBuilder,
        configuration: BuildConfiguration,
    }

    fn fixture(root_name: &str, engine: LatexEngine) -> Fixture {
        let project = tempdir().unwrap();
        fs::write(project.path().join(root_name), "\\documentclass{article}\n").unwrap();
        let state = tempdir().unwrap();
        let mut projects = ProjectService::default();
        let summary = projects.open(project.path()).unwrap();
        let trust = TrustService::load(state.path().join("trust.json")).unwrap();
        let builder = LatexmkRequestBuilder::new(state.path().join("builds")).unwrap();
        let configuration = BuildConfiguration {
            api_version: API_VERSION,
            project_id: summary.project_id,
            root_document: root_name.to_owned(),
            root_provenance: ResolutionProvenance::Detected,
            engine,
            engine_provenance: ResolutionProvenance::Default,
        };
        Fixture {
            _project: project,
            _state: state,
            projects,
            trust,
            builder,
            configuration,
        }
    }

    fn arguments(request: &LatexmkRequest) -> Vec<String> {
        request
            .arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn restricted_request_is_fixed_inspectable_and_bibliography_compatible() {
        for (engine, expected) in [
            (LatexEngine::PdfLatex, "-pdf"),
            (LatexEngine::XeLatex, "-xelatex"),
            (LatexEngine::LuaLatex, "-lualatex"),
        ] {
            let fixture = fixture("main.tex", engine);
            let request = fixture
                .builder
                .build(&fixture.projects, &fixture.trust, fixture.configuration)
                .unwrap();
            let args = arguments(&request);
            assert_eq!(request.executable_name, LATEXMK_EXECUTABLE);
            assert_eq!(request.working_directory, fixture._project.path());
            assert!(args.contains(&"-norc".to_owned()));
            assert!(args.contains(&expected.to_owned()));
            assert!(args.contains(&"-synctex=1".to_owned()));
            assert!(args.contains(&"-recorder".to_owned()));
            assert!(args.contains(&"-bibtex".to_owned()));
            assert!(args.contains(&"-no-shell-escape".to_owned()));
            assert_eq!(args.last().unwrap(), "./main.tex");
            assert!(!request.project_rc_enabled);
            assert!(!request.shell_escape_enabled);
            assert!(
                request
                    .artifacts
                    .pdf
                    .starts_with(&request.artifacts.directory)
            );
        }
    }

    #[test]
    fn risky_flags_require_independent_backend_permissions() {
        let mut fixture = fixture("main.tex", LatexEngine::PdfLatex);
        fs::write(fixture._project.path().join(".latexmkrc"), "$silent = 1;\n").unwrap();
        fixture
            .trust
            .set(
                &fixture.configuration.project_id,
                BuildPermission::LatexmkRc,
                true,
            )
            .unwrap();
        let request = fixture
            .builder
            .build(
                &fixture.projects,
                &fixture.trust,
                fixture.configuration.clone(),
            )
            .unwrap();
        let args = arguments(&request);
        assert!(request.project_rc_enabled);
        assert!(!request.shell_escape_enabled);
        assert_eq!(
            &args[..3],
            &["-norc".to_owned(), "-r".to_owned(), ".latexmkrc".to_owned()]
        );
        assert!(args.contains(&"-no-shell-escape".to_owned()));

        fixture
            .trust
            .set(
                &fixture.configuration.project_id,
                BuildPermission::ShellEscape,
                true,
            )
            .unwrap();
        let request = fixture
            .builder
            .build(&fixture.projects, &fixture.trust, fixture.configuration)
            .unwrap();
        let args = arguments(&request);
        assert!(request.shell_escape_enabled);
        assert!(args.contains(&"-shell-escape".to_owned()));
        assert!(!args.contains(&"-no-shell-escape".to_owned()));
    }

    #[test]
    fn permission_does_not_invent_a_missing_rc_and_outside_symlink_is_rejected() {
        let mut fixture = fixture("main.tex", LatexEngine::PdfLatex);
        fixture
            .trust
            .set(
                &fixture.configuration.project_id,
                BuildPermission::LatexmkRc,
                true,
            )
            .unwrap();
        let request = fixture
            .builder
            .build(
                &fixture.projects,
                &fixture.trust,
                fixture.configuration.clone(),
            )
            .unwrap();
        assert!(!request.project_rc_enabled);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = fixture._state.path().join("outside.rc");
            fs::write(&outside, "$silent = 1;").unwrap();
            symlink(outside, fixture._project.path().join(".latexmkrc")).unwrap();
            assert!(matches!(
                fixture
                    .builder
                    .build(&fixture.projects, &fixture.trust, fixture.configuration),
                Err(BuildRequestError::UnsafeProjectRc)
            ));
        }
    }

    #[test]
    fn artifact_validation_accepts_only_regular_files_in_the_build_cache() {
        let fixture = fixture("main.tex", LatexEngine::PdfLatex);
        let request = fixture
            .builder
            .build(
                &fixture.projects,
                &fixture.trust,
                fixture.configuration.clone(),
            )
            .unwrap();
        fs::write(&request.artifacts.log, "log").unwrap();
        assert_eq!(
            fixture
                .builder
                .validate_artifact_file(&request.artifacts.log)
                .unwrap(),
            request.artifacts.log.canonicalize().unwrap()
        );
        let outside = fixture._state.path().join("outside.log");
        fs::write(&outside, "outside").unwrap();
        assert!(matches!(
            fixture.builder.validate_artifact_file(&outside),
            Err(BuildRequestError::InvalidBuildCache)
        ));
    }

    #[test]
    fn bounded_artifact_reads_reject_oversized_files() {
        let fixture = fixture("main.tex", LatexEngine::PdfLatex);
        let request = fixture
            .builder
            .build(
                &fixture.projects,
                &fixture.trust,
                fixture.configuration.clone(),
            )
            .unwrap();
        fs::write(&request.artifacts.pdf, b"12345").unwrap();
        assert_eq!(
            fixture
                .builder
                .read_bounded_artifact(&request.artifacts.pdf, 5)
                .unwrap(),
            b"12345"
        );
        assert!(matches!(
            fixture
                .builder
                .read_bounded_artifact(&request.artifacts.pdf, 4),
            Err(BuildRequestError::ArtifactTooLarge { max_bytes: 4 })
        ));
    }

    #[test]
    fn retained_pdf_is_an_immutable_bounded_operation_snapshot() {
        let fixture = fixture("main.tex", LatexEngine::PdfLatex);
        let request = fixture
            .builder
            .build(
                &fixture.projects,
                &fixture.trust,
                fixture.configuration.clone(),
            )
            .unwrap();
        fs::write(&request.artifacts.pdf, b"%PDF-1.7\nfirst pdf\n%%EOF\n").unwrap();
        let retained = fixture
            .builder
            .retain_pdf_artifact(
                &request.artifacts.pdf,
                &fixture.configuration.project_id,
                "build-00000000000000000001",
                32,
            )
            .unwrap();
        fs::write(&request.artifacts.pdf, b"%PDF-1.7\nsecond pdf\n%%EOF\n").unwrap();
        assert_eq!(
            fs::read(&retained).unwrap(),
            b"%PDF-1.7\nfirst pdf\n%%EOF\n"
        );
        assert!(retained.ends_with("published/build-00000000000000000001.pdf"));
        fs::write(&request.artifacts.synctex, b"compressed synctex").unwrap();
        let retained_synctex = fixture
            .builder
            .retain_synctex_artifact(
                &request.artifacts.synctex,
                &fixture.configuration.project_id,
                "build-00000000000000000001",
                32,
            )
            .unwrap();
        assert!(retained_synctex.ends_with("published/build-00000000000000000001.synctex.gz"));
        assert_eq!(fs::read(retained_synctex).unwrap(), b"compressed synctex");
        assert!(matches!(
            fixture.builder.retain_pdf_artifact(
                &request.artifacts.pdf,
                &fixture.configuration.project_id,
                "../../escape",
                32,
            ),
            Err(BuildRequestError::InvalidOperationId)
        ));
        fs::write(&request.artifacts.pdf, b"%PDF-1.7\ntruncated").unwrap();
        assert!(matches!(
            fixture.builder.retain_pdf_artifact(
                &request.artifacts.pdf,
                &fixture.configuration.project_id,
                "build-00000000000000000002",
                32,
            ),
            Err(BuildRequestError::InvalidPdfArtifact)
        ));
    }

    #[test]
    fn clean_is_confined_to_the_exact_project_cache() {
        let fixture = fixture("main.tex", LatexEngine::PdfLatex);
        let request = fixture
            .builder
            .build(
                &fixture.projects,
                &fixture.trust,
                fixture.configuration.clone(),
            )
            .unwrap();
        fs::write(request.artifacts.directory.join("main.aux"), "generated").unwrap();
        let sibling = fixture._state.path().join("keep.txt");
        fs::write(&sibling, "keep").unwrap();
        fixture
            .builder
            .clean(&fixture.configuration.project_id)
            .unwrap();
        assert!(!request.artifacts.directory.exists());
        assert!(sibling.exists());
        assert!(matches!(
            fixture.builder.clean("../outside"),
            Err(BuildRequestError::Project(_))
        ));
    }

    #[test]
    fn root_filename_cannot_become_an_option_or_shell_fragment() {
        for root in [
            "-interaction=batchmode.tex",
            "bad%name.tex",
            "bad\nname.tex",
        ] {
            let fixture = fixture(root, LatexEngine::PdfLatex);
            let result =
                fixture
                    .builder
                    .build(&fixture.projects, &fixture.trust, fixture.configuration);
            if root.starts_with('-') {
                let args = arguments(&result.unwrap());
                assert_eq!(args.last().unwrap(), "./-interaction=batchmode.tex");
            } else {
                assert!(matches!(
                    result,
                    Err(BuildRequestError::InvalidRootFilename)
                ));
            }
        }
    }
}
