use crate::toolchain::{
    MANIFEST_VERSION, REQUIRED_BINARIES, ToolchainManifest, probe_version, supported_platform,
    validate_digest, validate_id,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};
use tempfile::{NamedTempFile, tempdir_in};
use thiserror::Error;

const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledToolchain {
    pub toolchain_id: String,
    pub version_root: PathBuf,
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("payload is unavailable or unreadable: {0}")]
    PayloadIo(#[source] io::Error),
    #[error("payload must be a regular file")]
    PayloadType,
    #[error("payload exceeds the 2 GiB compressed limit")]
    PayloadTooLarge,
    #[error("payload SHA-256 digest is invalid")]
    InvalidExpectedDigest,
    #[error("payload SHA-256 verification failed")]
    DigestMismatch,
    #[error("managed toolchain storage is unavailable: {0}")]
    Storage(#[source] io::Error),
    #[error("payload archive is invalid: {0}")]
    Archive(String),
    #[error("toolchain manifest is invalid: {0}")]
    Manifest(String),
    #[error("toolchain version already exists")]
    AlreadyExists,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActiveToolchain<'a> {
    format_version: u16,
    toolchain_id: &'a str,
}

pub fn install_offline_payload(
    managed_root: &Path,
    payload: &Path,
    expected_sha256: &str,
) -> Result<InstalledToolchain, InstallError> {
    validate_digest(expected_sha256).map_err(|_| InstallError::InvalidExpectedDigest)?;
    let path_metadata = fs::symlink_metadata(payload).map_err(InstallError::PayloadIo)?;
    if !path_metadata.file_type().is_file() {
        return Err(InstallError::PayloadType);
    }
    let mut payload_file = File::open(payload).map_err(InstallError::PayloadIo)?;
    let metadata = payload_file.metadata().map_err(InstallError::PayloadIo)?;
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(InstallError::PayloadTooLarge);
    }
    if hash_reader(&mut payload_file).map_err(InstallError::PayloadIo)?
        != expected_sha256.to_ascii_lowercase()
    {
        return Err(InstallError::DigestMismatch);
    }
    payload_file
        .seek(SeekFrom::Start(0))
        .map_err(InstallError::PayloadIo)?;

    fs::create_dir_all(managed_root).map_err(InstallError::Storage)?;
    let managed_root = managed_root.canonicalize().map_err(InstallError::Storage)?;
    let versions = managed_root.join("versions");
    fs::create_dir_all(&versions).map_err(InstallError::Storage)?;
    let versions = versions.canonicalize().map_err(InstallError::Storage)?;
    if !versions.starts_with(&managed_root) {
        return Err(InstallError::Archive(
            "versions directory escapes managed storage".to_owned(),
        ));
    }

    let staging = tempdir_in(&versions).map_err(InstallError::Storage)?;
    extract_payload(payload_file, staging.path())?;
    let manifest = validate_staged_toolchain(staging.path())?;
    let destination = versions.join(&manifest.toolchain_id);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(InstallError::AlreadyExists);
    }

    let staged_path = staging.keep();
    if let Err(error) = fs::rename(&staged_path, &destination) {
        let _ = fs::remove_dir_all(&staged_path);
        return Err(InstallError::Storage(error));
    }
    if let Err(error) = seal_tree(&destination) {
        let _ = make_tree_removable(&destination);
        let _ = fs::remove_dir_all(&destination);
        return Err(InstallError::Storage(error));
    }
    if let Err(error) = write_active(&managed_root, &manifest.toolchain_id) {
        let _ = make_tree_removable(&destination);
        let _ = fs::remove_dir_all(&destination);
        return Err(error);
    }

    Ok(InstalledToolchain {
        toolchain_id: manifest.toolchain_id,
        version_root: destination,
    })
}

fn hash_file(path: &Path) -> io::Result<String> {
    hash_reader(&mut BufReader::new(File::open(path)?))
}

fn hash_reader(reader: &mut impl Read) -> io::Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn extract_payload(payload: File, destination: &Path) -> Result<(), InstallError> {
    let decoder = zstd::Decoder::new(BufReader::new(payload))
        .map_err(|error| InstallError::Archive(error.to_string()))?;
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| InstallError::Archive(error.to_string()))?;
    let mut count = 0_usize;
    let mut expanded = 0_u64;
    let mut paths = BTreeSet::new();

    for entry in entries {
        count += 1;
        if count > MAX_ENTRIES {
            return Err(InstallError::Archive("entry limit exceeded".to_owned()));
        }
        let mut entry = entry.map_err(|error| InstallError::Archive(error.to_string()))?;
        let size = entry.size();
        if size > MAX_ENTRY_BYTES {
            return Err(InstallError::Archive(
                "entry size limit exceeded".to_owned(),
            ));
        }
        expanded = expanded
            .checked_add(size)
            .filter(|value| *value <= MAX_EXPANDED_BYTES)
            .ok_or_else(|| InstallError::Archive("expanded size limit exceeded".to_owned()))?;
        let relative = entry
            .path()
            .map_err(|error| InstallError::Archive(error.to_string()))?
            .into_owned();
        validate_archive_path(&relative)?;
        if !paths.insert(relative.clone()) {
            return Err(InstallError::Archive("duplicate entry path".to_owned()));
        }
        let target = destination.join(&relative);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&target).map_err(InstallError::Storage)?;
            continue;
        }
        if !entry_type.is_file() {
            return Err(InstallError::Archive(
                "links and special files are forbidden".to_owned(),
            ));
        }
        let parent = target
            .parent()
            .ok_or_else(|| InstallError::Archive("entry has no parent".to_owned()))?;
        fs::create_dir_all(parent).map_err(InstallError::Storage)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(InstallError::Storage)?;
        io::copy(&mut entry, &mut output).map_err(InstallError::Storage)?;
        output.flush().map_err(InstallError::Storage)?;
        output.sync_all().map_err(InstallError::Storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = entry
                .header()
                .mode()
                .map_err(|error| InstallError::Archive(error.to_string()))?
                & 0o777;
            fs::set_permissions(&target, fs::Permissions::from_mode(mode))
                .map_err(InstallError::Storage)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn seal_tree(root: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fn seal(path: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                seal(&entry?.path())?;
            }
        }
        let mode = if metadata.is_dir() {
            0o555
        } else {
            metadata.permissions().mode() & 0o555
        };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    seal(root)
}

#[cfg(not(unix))]
fn seal_tree(root: &Path) -> io::Result<()> {
    fn seal(path: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                seal(&entry?.path())?;
            }
        } else {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(path, permissions)?;
        }
        Ok(())
    }
    seal(root)
}

#[cfg(unix)]
fn make_tree_removable(root: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            make_tree_removable(&path)?;
        }
    }
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn make_tree_removable(root: &Path) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions)?;
        if path.is_dir() {
            make_tree_removable(&path)?;
        }
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<(), InstallError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        Err(InstallError::Archive(
            "entry path is absolute or contains traversal".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn validate_staged_toolchain(root: &Path) -> Result<ToolchainManifest, InstallError> {
    let bytes = fs::read(root.join("manifest.json")).map_err(InstallError::Storage)?;
    let manifest: ToolchainManifest = serde_json::from_slice(&bytes)
        .map_err(|error| InstallError::Manifest(error.to_string()))?;
    if manifest.format_version != MANIFEST_VERSION {
        return Err(InstallError::Manifest(
            "unsupported manifest version".to_owned(),
        ));
    }
    validate_id(&manifest.toolchain_id).map_err(InstallError::Manifest)?;
    if manifest.texlive_year != 2026 || manifest.platform != supported_platform() {
        return Err(InstallError::Manifest(
            "unsupported TeX Live release or platform".to_owned(),
        ));
    }
    let required = REQUIRED_BINARIES.into_iter().collect::<BTreeSet<_>>();
    if manifest
        .binaries
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != required
    {
        return Err(InstallError::Manifest(
            "binary set is incomplete or contains unknown entries".to_owned(),
        ));
    }

    let canonical_root = root.canonicalize().map_err(InstallError::Storage)?;
    for name in REQUIRED_BINARIES {
        let executable = root.join("bin").join(&manifest.platform).join(name);
        let resolved = executable.canonicalize().map_err(InstallError::Storage)?;
        if !resolved.starts_with(&canonical_root) || !resolved.is_file() {
            return Err(InstallError::Manifest(format!(
                "managed executable {name} escapes the payload"
            )));
        }
        let expected = manifest.binaries.get(name).expect("validated binary set");
        validate_digest(expected).map_err(InstallError::Manifest)?;
        let actual = hash_file(&resolved).map_err(InstallError::Storage)?;
        if actual != expected.to_ascii_lowercase() {
            return Err(InstallError::Manifest(format!(
                "managed executable {name} failed integrity verification"
            )));
        }
        probe_version(&resolved, root).map_err(|error| {
            InstallError::Manifest(format!(
                "managed executable {name} is not runnable: {error}"
            ))
        })?;
    }
    Ok(manifest)
}

fn write_active(root: &Path, toolchain_id: &str) -> Result<(), InstallError> {
    let bytes = serde_json::to_vec_pretty(&ActiveToolchain {
        format_version: MANIFEST_VERSION,
        toolchain_id,
    })
    .map_err(|error| InstallError::Manifest(error.to_string()))?;
    let mut temporary = NamedTempFile::new_in(root).map_err(InstallError::Storage)?;
    temporary.write_all(&bytes).map_err(InstallError::Storage)?;
    temporary.flush().map_err(InstallError::Storage)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(InstallError::Storage)?;
    temporary
        .persist(root.join("active.json"))
        .map_err(|error| InstallError::Storage(error.error))?;
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(InstallError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchain::ToolchainService;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use tar::{Builder, EntryType, Header};
    use tempfile::tempdir;

    fn append(builder: &mut Builder<zstd::Encoder<'_, File>>, path: &str, bytes: &[u8], mode: u32) {
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(mode);
        header.set_entry_type(EntryType::Regular);
        header.set_cksum();
        builder.append_data(&mut header, path, bytes).unwrap();
    }

    fn payload(directory: &Path, include_link: bool) -> (PathBuf, String) {
        let archive_path = directory.join("texlive.tar.zst");
        let encoder = zstd::Encoder::new(File::create(&archive_path).unwrap(), 1).unwrap();
        let mut builder = Builder::new(encoder);
        let script = b"#!/bin/sh\nprintf 'fixture 1.0\\n'\n";
        let digest = format!("{:x}", Sha256::digest(script));
        let binaries = REQUIRED_BINARIES
            .into_iter()
            .map(|name| (name, digest.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let manifest = serde_json::to_vec(&json!({
            "formatVersion": 1,
            "toolchainId": "texlive-2026.0-x86_64-linux",
            "texliveYear": 2026,
            "texliveRevision": 80315,
            "platform": "x86_64-linux",
            "binaries": binaries,
        }))
        .unwrap();
        append(&mut builder, "manifest.json", &manifest, 0o644);
        for name in REQUIRED_BINARIES {
            append(
                &mut builder,
                &format!("bin/x86_64-linux/{name}"),
                script,
                0o755,
            );
        }
        if include_link {
            let mut header = Header::new_gnu();
            header.set_entry_type(EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_link_name("../../outside").unwrap();
            header.set_cksum();
            builder
                .append_data(&mut header, "escape", io::empty())
                .unwrap();
        }
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
        let archive_digest = hash_file(&archive_path).unwrap();
        (archive_path, archive_digest)
    }

    #[test]
    fn installs_verifies_and_atomically_activates_payload() {
        let temp = tempdir().unwrap();
        let (archive, digest) = payload(temp.path(), false);
        let managed = temp.path().join("managed");
        let installed = install_offline_payload(&managed, &archive, &digest).unwrap();
        assert_eq!(installed.toolchain_id, "texlive-2026.0-x86_64-linux");
        assert!(installed.version_root.join("manifest.json").is_file());
        assert_eq!(
            ToolchainService::new(managed).readiness().status,
            crate::api::ToolchainStatus::Ready
        );
    }

    #[test]
    fn rejects_digest_mismatch_without_creating_storage() {
        let temp = tempdir().unwrap();
        let (archive, _) = payload(temp.path(), false);
        let managed = temp.path().join("managed");
        let error = install_offline_payload(&managed, &archive, &"0".repeat(64)).unwrap_err();
        assert!(matches!(error, InstallError::DigestMismatch));
        assert!(!managed.exists());
    }

    #[test]
    fn rejects_links_without_replacing_the_active_version() {
        let temp = tempdir().unwrap();
        let managed = temp.path().join("managed");
        fs::create_dir_all(&managed).unwrap();
        fs::write(
            managed.join("active.json"),
            br#"{"formatVersion":1,"toolchainId":"known-good"}"#,
        )
        .unwrap();
        let (archive, digest) = payload(temp.path(), true);
        let error = install_offline_payload(&managed, &archive, &digest).unwrap_err();
        assert!(matches!(error, InstallError::Archive(_)));
        assert_eq!(
            fs::read_to_string(managed.join("active.json")).unwrap(),
            r#"{"formatVersion":1,"toolchainId":"known-good"}"#
        );
        assert!(!temp.path().join("outside").exists());
    }

    #[test]
    fn installed_version_is_read_only() {
        let temp = tempdir().unwrap();
        let (archive, digest) = payload(temp.path(), false);
        let installed =
            install_offline_payload(&temp.path().join("managed"), &archive, &digest).unwrap();
        let mode = fs::metadata(installed.version_root.join("bin/x86_64-linux/latexmk"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o222, 0);
        assert_eq!(
            fs::metadata(&installed.version_root)
                .unwrap()
                .permissions()
                .mode()
                & 0o222,
            0
        );
    }
}
