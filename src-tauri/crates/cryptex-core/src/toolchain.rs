use crate::api::{API_VERSION, ToolchainBinary, ToolchainReadiness, ToolchainStatus};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MANIFEST_VERSION: u16 = 1;
const REQUIRED_BINARIES: [&str; 8] = [
    "latexmk",
    "pdflatex",
    "xelatex",
    "lualatex",
    "bibtex",
    "biber",
    "kpsewhich",
    "synctex",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActiveToolchain {
    format_version: u16,
    toolchain_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ToolchainManifest {
    format_version: u16,
    toolchain_id: String,
    texlive_year: u16,
    texlive_revision: u32,
    platform: String,
    binaries: BTreeMap<String, String>,
}

pub struct ToolchainService {
    root: PathBuf,
}

impl ToolchainService {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn readiness(&self) -> ToolchainReadiness {
        self.inspect().unwrap_or_else(|message| ToolchainReadiness {
            api_version: API_VERSION,
            status: ToolchainStatus::Corrupt,
            toolchain_id: None,
            texlive_year: None,
            texlive_revision: None,
            platform: None,
            binaries: Vec::new(),
            message: Some(message),
            repairable: true,
        })
    }

    pub fn verified_executable(&self, name: &str) -> Result<PathBuf, String> {
        if !REQUIRED_BINARIES.contains(&name) {
            return Err("requested executable is not part of the managed toolchain".to_owned());
        }
        let readiness = self.inspect()?;
        if readiness.status != ToolchainStatus::Ready {
            return Err(readiness
                .message
                .unwrap_or_else(|| "managed toolchain is not ready".to_owned()));
        }
        let active: ActiveToolchain = serde_json::from_slice(
            &fs::read(self.root.join("active.json"))
                .map_err(|error| format!("unable to reread active toolchain: {error}"))?,
        )
        .map_err(|error| format!("active toolchain record is invalid: {error}"))?;
        validate_version(active.format_version, "active record")?;
        validate_id(&active.toolchain_id)?;
        let managed_root = self
            .root
            .canonicalize()
            .map_err(|error| format!("managed toolchain root is unavailable: {error}"))?;
        let version_root = self
            .root
            .join("versions")
            .join(&active.toolchain_id)
            .canonicalize()
            .map_err(|error| format!("active toolchain directory is unavailable: {error}"))?;
        if !version_root.starts_with(&managed_root) {
            return Err("active toolchain resolves outside the managed root".to_owned());
        }
        let manifest: ToolchainManifest = serde_json::from_slice(
            &fs::read(version_root.join("manifest.json"))
                .map_err(|error| format!("unable to reread toolchain manifest: {error}"))?,
        )
        .map_err(|error| format!("toolchain manifest is invalid: {error}"))?;
        if manifest.toolchain_id != active.toolchain_id || manifest.platform != supported_platform()
        {
            return Err("active toolchain identity changed during verification".to_owned());
        }
        let executable = version_root
            .join("bin")
            .join(&manifest.platform)
            .join(name)
            .canonicalize()
            .map_err(|error| format!("managed executable {name} is unavailable: {error}"))?;
        if !executable.starts_with(&version_root) || !executable.is_file() {
            return Err(format!(
                "managed executable {name} escapes the toolchain root"
            ));
        }
        let expected = manifest
            .binaries
            .get(name)
            .ok_or_else(|| format!("managed executable {name} is absent from the manifest"))?;
        validate_digest(expected)?;
        let actual =
            format!(
                "{:x}",
                Sha256::digest(fs::read(&executable).map_err(|error| format!(
                    "unable to hash managed executable {name}: {error}"
                ))?)
            );
        if actual != *expected {
            return Err(format!(
                "managed executable {name} failed integrity verification"
            ));
        }
        Ok(executable)
    }

    fn inspect(&self) -> Result<ToolchainReadiness, String> {
        let bytes = match fs::read(self.root.join("active.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(missing()),
            Err(error) => return Err(format!("unable to read active toolchain: {error}")),
        };
        let active: ActiveToolchain = serde_json::from_slice(&bytes)
            .map_err(|error| format!("active toolchain record is invalid: {error}"))?;
        validate_version(active.format_version, "active record")?;
        validate_id(&active.toolchain_id)?;

        let managed_root = self
            .root
            .canonicalize()
            .map_err(|error| format!("managed toolchain root is unavailable: {error}"))?;
        let versions = self
            .root
            .join("versions")
            .canonicalize()
            .map_err(|error| format!("toolchain versions directory is unavailable: {error}"))?;
        if !versions.starts_with(&managed_root) {
            return Err("toolchain versions directory escapes the managed root".to_owned());
        }
        let version_root = versions
            .join(&active.toolchain_id)
            .canonicalize()
            .map_err(|error| format!("active toolchain directory is unavailable: {error}"))?;
        if !version_root.starts_with(&versions) {
            return Err("active toolchain resolves outside the managed root".to_owned());
        }

        let manifest: ToolchainManifest = serde_json::from_slice(
            &fs::read(version_root.join("manifest.json"))
                .map_err(|error| format!("unable to read toolchain manifest: {error}"))?,
        )
        .map_err(|error| format!("toolchain manifest is invalid: {error}"))?;
        validate_version(manifest.format_version, "manifest")?;
        if manifest.toolchain_id != active.toolchain_id {
            return Err("manifest identity does not match the active record".to_owned());
        }
        if manifest.texlive_year != 2026 {
            return Err("toolchain is not the supported TeX Live 2026 release".to_owned());
        }
        if manifest.platform != supported_platform() {
            return Ok(incompatible(&manifest));
        }
        let required = REQUIRED_BINARIES.into_iter().collect::<BTreeSet<_>>();
        if manifest
            .binaries
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != required
        {
            return Err("manifest binary set is incomplete or contains unknown entries".to_owned());
        }

        let bin_root = version_root.join("bin").join(&manifest.platform);
        let mut binaries = Vec::new();
        for name in REQUIRED_BINARIES {
            let resolved = bin_root
                .join(name)
                .canonicalize()
                .map_err(|error| format!("managed executable {name} is unavailable: {error}"))?;
            if !resolved.starts_with(&version_root) || !resolved.is_file() {
                return Err(format!(
                    "managed executable {name} escapes the toolchain root"
                ));
            }
            let expected = manifest.binaries.get(name).expect("validated binary set");
            validate_digest(expected)?;
            let actual = format!(
                "{:x}",
                Sha256::digest(fs::read(&resolved).map_err(|error| format!(
                    "unable to hash managed executable {name}: {error}"
                ))?)
            );
            if actual != *expected {
                return Err(format!(
                    "managed executable {name} failed integrity verification"
                ));
            }
            let version = probe_version(&resolved, &version_root)
                .map_err(|error| format!("managed executable {name} is not runnable: {error}"))?;
            binaries.push(ToolchainBinary {
                name: name.to_owned(),
                version,
            });
        }

        Ok(ToolchainReadiness {
            api_version: API_VERSION,
            status: ToolchainStatus::Ready,
            toolchain_id: Some(manifest.toolchain_id),
            texlive_year: Some(manifest.texlive_year),
            texlive_revision: Some(manifest.texlive_revision),
            platform: Some(manifest.platform),
            binaries,
            message: None,
            repairable: false,
        })
    }
}

fn missing() -> ToolchainReadiness {
    ToolchainReadiness {
        api_version: API_VERSION,
        status: ToolchainStatus::Missing,
        toolchain_id: None,
        texlive_year: None,
        texlive_revision: None,
        platform: Some(supported_platform().to_owned()),
        binaries: Vec::new(),
        message: Some("Managed TeX Live is not installed".to_owned()),
        repairable: true,
    }
}

fn incompatible(manifest: &ToolchainManifest) -> ToolchainReadiness {
    ToolchainReadiness {
        api_version: API_VERSION,
        status: ToolchainStatus::Incompatible,
        toolchain_id: Some(manifest.toolchain_id.clone()),
        texlive_year: Some(manifest.texlive_year),
        texlive_revision: Some(manifest.texlive_revision),
        platform: Some(manifest.platform.clone()),
        binaries: Vec::new(),
        message: Some("toolchain platform is incompatible".to_owned()),
        repairable: true,
    }
}

fn supported_platform() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-linux"
    } else {
        "unsupported"
    }
}

fn validate_version(version: u16, label: &str) -> Result<(), String> {
    if version == MANIFEST_VERSION {
        Ok(())
    } else {
        Err(format!("unsupported {label} version {version}"))
    }
}

fn validate_id(value: &str) -> Result<(), String> {
    if !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err("toolchain identity is invalid".to_owned())
    }
}

fn validate_digest(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("manifest contains an invalid SHA-256 digest".to_owned())
    }
}

fn probe_version(executable: &Path, root: &Path) -> Result<String, String> {
    let output = Command::new(executable)
        .arg("--version")
        .current_dir(root)
        .env_clear()
        .env("PATH", executable.parent().unwrap_or(root))
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(300)
        .collect::<String>();
    if line.is_empty() {
        Err("version probe returned no output".to_owned())
    } else {
        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn fixture() -> (tempfile::TempDir, ToolchainService) {
        let temp = tempdir().unwrap();
        let root = temp.path().join("toolchains");
        let id = "texlive-2026.0-x86_64-linux";
        let version = root.join("versions").join(id);
        let bin = version.join("bin/x86_64-linux");
        fs::create_dir_all(&bin).unwrap();
        let mut binaries = BTreeMap::new();
        for name in REQUIRED_BINARIES {
            let path = bin.join(name);
            fs::write(&path, format!("#!/bin/sh\necho '{name} 2026'\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            binaries.insert(
                name,
                format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            );
        }
        fs::write(version.join("manifest.json"), serde_json::to_vec(&serde_json::json!({
            "formatVersion": 1, "toolchainId": id, "texliveYear": 2026, "texliveRevision": 80315,
            "platform": "x86_64-linux", "binaries": binaries,
        })).unwrap()).unwrap();
        fs::write(
            root.join("active.json"),
            format!(r#"{{"formatVersion":1,"toolchainId":"{id}"}}"#),
        )
        .unwrap();
        let service = ToolchainService::new(root);
        (temp, service)
    }

    #[test]
    fn missing_installation_is_actionable() {
        let temp = tempdir().unwrap();
        let status = ToolchainService::new(temp.path().join("none")).readiness();
        assert_eq!(status.status, ToolchainStatus::Missing);
        assert!(status.repairable);
    }

    #[test]
    fn verifies_every_binary_before_reporting_ready() {
        let (_temp, service) = fixture();
        let status = service.readiness();
        assert_eq!(
            status.status,
            ToolchainStatus::Ready,
            "{:?}",
            status.message
        );
        assert_eq!(status.binaries.len(), REQUIRED_BINARIES.len());
        assert_eq!(status.texlive_revision, Some(80315));
    }

    #[test]
    fn resolves_only_a_freshly_verified_manifest_executable() {
        let (_temp, service) = fixture();
        let executable = service.verified_executable("latexmk").unwrap();
        assert_eq!(
            executable.file_name().and_then(|name| name.to_str()),
            Some("latexmk")
        );
        assert!(service.verified_executable("sh").is_err());
    }

    #[test]
    fn rejects_tampering_and_unsafe_active_identity() {
        let (temp, service) = fixture();
        fs::write(
            temp.path()
                .join("toolchains/versions/texlive-2026.0-x86_64-linux/bin/x86_64-linux/latexmk"),
            "tampered",
        )
        .unwrap();
        assert_eq!(service.readiness().status, ToolchainStatus::Corrupt);
        fs::write(
            temp.path().join("toolchains/active.json"),
            r#"{"formatVersion":1,"toolchainId":"../../outside"}"#,
        )
        .unwrap();
        assert_eq!(service.readiness().status, ToolchainStatus::Corrupt);
    }
}
