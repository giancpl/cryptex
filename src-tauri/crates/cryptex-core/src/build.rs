use crate::{
    api::{API_VERSION, BuildConfiguration, LatexEngine, ResolutionProvenance, RootDocumentReason},
    project::{ProjectError, ProjectService},
};
use thiserror::Error;

pub fn resolve_build_configuration(
    projects: &ProjectService,
    project_id: &str,
    preferred_root: Option<&str>,
    preferred_engine: Option<LatexEngine>,
) -> Result<BuildConfiguration, BuildResolutionError> {
    let roots = projects.detect_root_documents(project_id, preferred_root)?;
    let root_document = roots.selected.ok_or(match roots.candidates.len() {
        0 => BuildResolutionError::NoRootDocument,
        _ => BuildResolutionError::AmbiguousRootDocument,
    })?;
    let candidate = roots
        .candidates
        .iter()
        .find(|candidate| candidate.relative_path == root_document)
        .ok_or(BuildResolutionError::NoRootDocument)?;
    let root_provenance = if candidate.reasons.contains(&RootDocumentReason::Preferred) {
        ResolutionProvenance::ProjectPreference
    } else if candidate.reasons.contains(&RootDocumentReason::MagicRoot) {
        ResolutionProvenance::MagicComment
    } else {
        ResolutionProvenance::Detected
    };

    let (engine, engine_provenance) = if let Some(engine) = preferred_engine {
        (engine, ResolutionProvenance::ProjectPreference)
    } else {
        let document = projects.read_text_file(project_id, &root_document)?;
        match magic_engine(&document.text)? {
            Some(engine) => (engine, ResolutionProvenance::MagicComment),
            None => (LatexEngine::PdfLatex, ResolutionProvenance::Default),
        }
    };

    Ok(BuildConfiguration {
        api_version: API_VERSION,
        project_id: project_id.to_owned(),
        root_document,
        root_provenance,
        engine,
        engine_provenance,
    })
}

fn magic_engine(text: &str) -> Result<Option<LatexEngine>, BuildResolutionError> {
    for line in text.lines().take(20) {
        let Some(comment) = line.trim_start().strip_prefix('%').map(str::trim) else {
            continue;
        };
        let Some((key, value)) = comment.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !key.eq_ignore_ascii_case("!TEX program") && !key.eq_ignore_ascii_case("!TEX TS-program")
        {
            continue;
        }
        let value = value.trim();
        let engine = match value.to_ascii_lowercase().as_str() {
            "pdflatex" => LatexEngine::PdfLatex,
            "xelatex" => LatexEngine::XeLatex,
            "lualatex" => LatexEngine::LuaLatex,
            _ => return Err(BuildResolutionError::UnsupportedEngine(value.to_owned())),
        };
        return Ok(Some(engine));
    }
    Ok(None)
}

#[derive(Debug, Error)]
pub enum BuildResolutionError {
    #[error("no LaTeX root document was detected")]
    NoRootDocument,
    #[error("multiple LaTeX root documents require an explicit project selection")]
    AmbiguousRootDocument,
    #[error("unsupported or unsafe TeX engine magic value: {0}")]
    UnsupportedEngine(String),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, ProjectService, String) {
        let directory = tempdir().unwrap();
        for (path, text) in files {
            let path = directory.path().join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, text).unwrap();
        }
        let mut projects = ProjectService::default();
        let summary = projects.open(directory.path()).unwrap();
        (directory, projects, summary.project_id)
    }

    #[test]
    fn defaults_single_detected_root_to_pdflatex() {
        let (_directory, projects, id) = project(&[("main.tex", "\\documentclass{article}\n")]);
        let configuration = resolve_build_configuration(&projects, &id, None, None).unwrap();
        assert_eq!(configuration.root_document, "main.tex");
        assert_eq!(
            configuration.root_provenance,
            ResolutionProvenance::Detected
        );
        assert_eq!(configuration.engine, LatexEngine::PdfLatex);
        assert_eq!(
            configuration.engine_provenance,
            ResolutionProvenance::Default
        );
    }

    #[test]
    fn supports_only_the_three_declared_magic_engines() {
        for (value, expected) in [
            ("pdflatex", LatexEngine::PdfLatex),
            ("XeLaTeX", LatexEngine::XeLatex),
            ("lualatex", LatexEngine::LuaLatex),
        ] {
            let source = format!("% !TEX TS-program = {value}\n\\documentclass{{article}}\n");
            let (_directory, projects, id) = project(&[("main.tex", &source)]);
            let configuration = resolve_build_configuration(&projects, &id, None, None).unwrap();
            assert_eq!(configuration.engine, expected);
            assert_eq!(
                configuration.engine_provenance,
                ResolutionProvenance::MagicComment
            );
        }
    }

    #[test]
    fn explicit_preferences_win_and_record_both_provenances() {
        let (_directory, projects, id) = project(&[
            (
                "a.tex",
                "% !TEX program = xelatex\n\\documentclass{article}\n",
            ),
            ("b.tex", "\\documentclass{book}\n"),
        ]);
        let configuration =
            resolve_build_configuration(&projects, &id, Some("b.tex"), Some(LatexEngine::LuaLatex))
                .unwrap();
        assert_eq!(configuration.root_document, "b.tex");
        assert_eq!(
            configuration.root_provenance,
            ResolutionProvenance::ProjectPreference
        );
        assert_eq!(configuration.engine, LatexEngine::LuaLatex);
        assert_eq!(
            configuration.engine_provenance,
            ResolutionProvenance::ProjectPreference
        );
    }

    #[test]
    fn rejects_command_fragments_and_unsupported_magic_engines() {
        for value in [
            "xelatex --shell-escape",
            "latex; touch owned",
            "",
            "tectonic",
        ] {
            let source = format!("% !TEX program = {value}\n\\documentclass{{article}}\n");
            let (_directory, projects, id) = project(&[("main.tex", &source)]);
            assert!(matches!(
                resolve_build_configuration(&projects, &id, None, None),
                Err(BuildResolutionError::UnsupportedEngine(_))
            ));
        }
    }

    #[test]
    fn refuses_ambiguous_or_missing_roots() {
        let (_directory, projects, id) = project(&[
            ("a.tex", "\\documentclass{article}\n"),
            ("b.tex", "\\documentclass{book}\n"),
        ]);
        assert!(matches!(
            resolve_build_configuration(&projects, &id, None, None),
            Err(BuildResolutionError::AmbiguousRootDocument)
        ));

        let (_directory, projects, id) = project(&[("chapter.tex", "text only\n")]);
        assert!(matches!(
            resolve_build_configuration(&projects, &id, None, None),
            Err(BuildResolutionError::NoRootDocument)
        ));
    }
}
