use crate::toolchain::{MANIFEST_VERSION, REQUIRED_BINARIES, supported_platform, validate_id};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

const MAX_EXPANDED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;
const INVENTORY_PATH: &str = "release/package-inventory.json";
const MANIFEST_PATH: &str = "manifest.json";
const REQUIRED_ROOTS: [&str; 12] = [
    "collection-basic",
    "collection-latex",
    "collection-latexrecommended",
    "collection-latexextra",
    "collection-fontsrecommended",
    "collection-bibtexextra",
    "collection-pictures",
    "collection-xetex",
    "collection-luatex",
    "latexmk",
    "cryptocode",
    "synctex",
];

#[derive(Debug, Clone)]
pub struct PayloadConfig {
    pub toolchain_id: String,
    pub texlive_revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadBuildResult {
    pub sha256: String,
    pub entries: usize,
    pub expanded_bytes: u64,
}

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("payload source is invalid: {0}")]
    Source(String),
    #[error("payload inventory is invalid: {0}")]
    Inventory(String),
    #[error("payload entry is unsafe: {0}")]
    UnsafeEntry(String),
    #[error("payload exceeds installer limits")]
    Limits,
    #[error("payload I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("payload serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InventoryHeader {
    format_version: u16,
    texlive_year: u16,
    texlive_revision: u32,
    platform: String,
    tlpdb_sha512: String,
    roots: Vec<String>,
    package_count: usize,
    packages_without_declared_license: Vec<String>,
    packages: Vec<InventoryPackage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InventoryPackage {
    name: String,
    license: Option<String>,
    dependencies: Vec<String>,
    archive_bytes: InventorySizes,
    installed_bytes: InventorySizes,
}

#[derive(Debug, Deserialize)]
struct InventorySizes {
    runtime: u64,
    documentation: u64,
    source: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PayloadManifest {
    format_version: u16,
    toolchain_id: String,
    texlive_year: u16,
    texlive_revision: u32,
    platform: String,
    binaries: BTreeMap<String, String>,
}

#[derive(Debug)]
struct SourceEntry {
    relative: PathBuf,
    source: PathBuf,
    directory: bool,
    mode: u32,
    size: u64,
}

pub fn build_payload(
    source_root: &Path,
    inventory_bytes: &[u8],
    config: &PayloadConfig,
    output: impl Write,
) -> Result<PayloadBuildResult, PackageError> {
    validate_id(&config.toolchain_id).map_err(PackageError::Source)?;
    let inventory: InventoryHeader = serde_json::from_slice(inventory_bytes)
        .map_err(|error| PackageError::Inventory(error.to_string()))?;
    validate_inventory(&inventory, config)?;

    let root = source_root
        .canonicalize()
        .map_err(|error| PackageError::Source(error.to_string()))?;
    if !root.is_dir() {
        return Err(PackageError::Source(
            "runtime root must be a directory".to_owned(),
        ));
    }
    let mut entries = Vec::new();
    collect_entries(&root, &root, &mut entries)?;
    entries.sort_by(|left, right| left.relative.cmp(&right.relative));
    validate_limits(&entries, inventory_bytes.len() as u64)?;

    let hashing = HashingWriter::new(output);
    let encoder = zstd::Encoder::new(hashing, 19)?;
    let mut archive = tar::Builder::new(encoder);
    archive.mode(tar::HeaderMode::Deterministic);
    let binary_paths = REQUIRED_BINARIES
        .into_iter()
        .map(|name| {
            (
                PathBuf::from("bin").join(supported_platform()).join(name),
                name,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut binary_hashes = BTreeMap::new();

    for entry in &entries {
        if entry.directory {
            append_directory(&mut archive, &entry.relative)?;
        } else {
            let digest = append_file(&mut archive, entry)?;
            if let Some(name) = binary_paths.get(&entry.relative) {
                binary_hashes.insert((*name).to_owned(), digest);
            }
        }
    }
    if binary_hashes.len() != REQUIRED_BINARIES.len() {
        let present = binary_hashes
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let missing = REQUIRED_BINARIES
            .into_iter()
            .filter(|name| !present.contains(name))
            .collect::<Vec<_>>();
        return Err(PackageError::Source(format!(
            "required executables are missing: {}",
            missing.join(", ")
        )));
    }
    append_bytes(&mut archive, INVENTORY_PATH, inventory_bytes, 0o444)?;
    let manifest = serde_json::to_vec_pretty(&PayloadManifest {
        format_version: MANIFEST_VERSION,
        toolchain_id: config.toolchain_id.clone(),
        texlive_year: 2026,
        texlive_revision: config.texlive_revision,
        platform: supported_platform().to_owned(),
        binaries: binary_hashes,
    })?;
    append_bytes(&mut archive, MANIFEST_PATH, &manifest, 0o444)?;
    archive.finish()?;
    let encoder = archive.into_inner()?;
    let hashing = encoder.finish()?;
    let digest = hashing.finish();
    let expanded_bytes = entries.iter().map(|entry| entry.size).sum::<u64>()
        + inventory_bytes.len() as u64
        + manifest.len() as u64;
    Ok(PayloadBuildResult {
        sha256: digest,
        entries: entries.len() + 2,
        expanded_bytes,
    })
}

fn validate_inventory(
    inventory: &InventoryHeader,
    config: &PayloadConfig,
) -> Result<(), PackageError> {
    let names = inventory
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    let mut sorted_names = names.clone();
    sorted_names.sort_unstable();
    let unlicensed = inventory
        .packages
        .iter()
        .filter(|package| package.license.is_none())
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    let roots = inventory
        .roots
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let records_are_sane = inventory.packages.iter().all(|package| {
        !package.name.is_empty()
            && package
                .dependencies
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            && package.archive_bytes.total().is_some()
            && package.installed_bytes.total().is_some()
    });
    if inventory.format_version != 1
        || inventory.texlive_year != 2026
        || inventory.texlive_revision != config.texlive_revision
        || inventory.platform != supported_platform()
        || inventory.tlpdb_sha512.len() != 128
        || !inventory
            .tlpdb_sha512
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || roots != REQUIRED_ROOTS
        || inventory.package_count == 0
        || inventory.package_count != inventory.packages.len()
        || names != sorted_names
        || names.windows(2).any(|pair| pair[0] == pair[1])
        || inventory
            .packages_without_declared_license
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != unlicensed
        || !records_are_sane
    {
        return Err(PackageError::Inventory(
            "version, revision, platform, or package count does not match".to_owned(),
        ));
    }
    Ok(())
}

impl InventorySizes {
    fn total(&self) -> Option<u64> {
        self.runtime
            .checked_add(self.documentation)?
            .checked_add(self.source)
    }
}

fn collect_entries(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<SourceEntry>,
) -> Result<(), PackageError> {
    let mut children = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| PackageError::Source(error.to_string()))?
            .to_owned();
        if relative == Path::new(MANIFEST_PATH) || relative == Path::new(INVENTORY_PATH) {
            return Err(PackageError::Source(format!(
                "reserved payload path already exists: {}",
                relative.display()
            )));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            entries.push(SourceEntry {
                relative,
                source: path.clone(),
                directory: true,
                mode: 0o555,
                size: 0,
            });
            collect_entries(root, &path, entries)?;
        } else if metadata.is_file() {
            entries.push(file_entry(relative, path, &metadata));
        } else if metadata.file_type().is_symlink() {
            let resolved = path.canonicalize().map_err(|error| {
                PackageError::UnsafeEntry(format!("{}: {error}", relative.display()))
            })?;
            if !resolved.starts_with(root) || !resolved.is_file() {
                return Err(PackageError::UnsafeEntry(format!(
                    "{} does not resolve to an in-root file",
                    relative.display()
                )));
            }
            let target_metadata = resolved.metadata()?;
            entries.push(file_entry(relative, resolved, &target_metadata));
        } else {
            return Err(PackageError::UnsafeEntry(format!(
                "{} is a special file",
                relative.display()
            )));
        }
    }
    Ok(())
}

fn file_entry(relative: PathBuf, source: PathBuf, metadata: &fs::Metadata) -> SourceEntry {
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    SourceEntry {
        relative,
        source,
        directory: false,
        mode: if executable { 0o555 } else { 0o444 },
        size: metadata.len(),
    }
}

fn validate_limits(entries: &[SourceEntry], inventory_bytes: u64) -> Result<(), PackageError> {
    if entries.len() + 2 > MAX_ENTRIES
        || inventory_bytes > MAX_ENTRY_BYTES
        || entries.iter().any(|entry| entry.size > MAX_ENTRY_BYTES)
    {
        return Err(PackageError::Limits);
    }
    let total = entries
        .iter()
        .try_fold(inventory_bytes, |total, entry| {
            total.checked_add(entry.size)
        })
        .filter(|total| *total <= MAX_EXPANDED_BYTES);
    if total.is_none() {
        return Err(PackageError::Limits);
    }
    Ok(())
}

fn append_directory(
    archive: &mut tar::Builder<zstd::Encoder<'_, HashingWriter<impl Write>>>,
    path: &Path,
) -> Result<(), PackageError> {
    let mut header = deterministic_header(0, 0o555, tar::EntryType::Directory);
    archive.append_data(&mut header, path, io::empty())?;
    Ok(())
}

fn append_file(
    archive: &mut tar::Builder<zstd::Encoder<'_, HashingWriter<impl Write>>>,
    entry: &SourceEntry,
) -> Result<String, PackageError> {
    let mut input = File::open(&entry.source)?;
    let mut digest = Sha256::new();
    let copied = io::copy(&mut input, &mut digest)?;
    if copied != entry.size {
        return Err(PackageError::Source(format!(
            "{} changed while packaging",
            entry.relative.display()
        )));
    }
    input.seek(SeekFrom::Start(0))?;
    let mut header = deterministic_header(entry.size, entry.mode, tar::EntryType::Regular);
    archive.append_data(&mut header, &entry.relative, &mut input)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn append_bytes(
    archive: &mut tar::Builder<zstd::Encoder<'_, HashingWriter<impl Write>>>,
    path: impl AsRef<Path>,
    bytes: &[u8],
    mode: u32,
) -> Result<(), PackageError> {
    let mut header = deterministic_header(bytes.len() as u64, mode, tar::EntryType::Regular);
    archive.append_data(&mut header, path, bytes)?;
    Ok(())
}

fn deterministic_header(size: u64, mode: u32, entry_type: tar::EntryType) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_size(size);
    header.set_mode(mode);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_entry_type(entry_type);
    header.set_cksum();
    header
}

struct HashingWriter<W> {
    inner: W,
    digest: Sha256,
}

impl<W> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            digest: Sha256::new(),
        }
    }

    fn finish(self) -> String {
        format!("{:x}", self.digest.finalize())
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.digest.update(&buffer[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchain_install::install_offline_payload;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use tempfile::tempdir;

    fn fixture(root: &Path) -> Vec<u8> {
        let bin = root.join("bin/x86_64-linux");
        fs::create_dir_all(&bin).unwrap();
        let script = b"#!/bin/sh\nprintf 'fixture 1.0\\n'\n";
        for name in REQUIRED_BINARIES {
            let path = bin.join(name);
            fs::write(&path, script).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::create_dir_all(root.join("texmf-dist/tex/plain")).unwrap();
        fs::write(root.join("texmf-dist/tex/plain/data.tex"), b"plain data").unwrap();
        symlink(
            root.join("texmf-dist/tex/plain/data.tex"),
            root.join("texmf-dist/tex/plain/alias.tex"),
        )
        .unwrap();
        serde_json::to_vec(&serde_json::json!({
            "formatVersion": 1,
            "texliveYear": 2026,
            "texliveRevision": 80315,
            "platform": "x86_64-linux",
            "tlpdbSha512": "a".repeat(128),
            "roots": REQUIRED_ROOTS,
            "packageCount": 1,
            "packagesWithoutDeclaredLicense": ["fixture"],
            "packages": [{
                "name": "fixture",
                "license": null,
                "dependencies": [],
                "archiveBytes": {"runtime": 1, "documentation": 0, "source": 0},
                "installedBytes": {"runtime": 1, "documentation": 0, "source": 0}
            }]
        }))
        .unwrap()
    }

    fn build(root: &Path, inventory: &[u8]) -> (Vec<u8>, PayloadBuildResult) {
        let mut output = Vec::new();
        let result = build_payload(
            root,
            inventory,
            &PayloadConfig {
                toolchain_id: "texlive-2026.0-x86_64-linux".to_owned(),
                texlive_revision: 80315,
            },
            &mut output,
        )
        .unwrap();
        (output, result)
    }

    #[test]
    fn produces_identical_installable_archives_and_materializes_links() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let inventory = fixture(&source);
        let (first, first_result) = build(&source, &inventory);
        let (second, second_result) = build(&source, &inventory);
        assert_eq!(first, second);
        assert_eq!(first_result.sha256, second_result.sha256);

        let payload = temp.path().join("payload.tar.zst");
        fs::write(&payload, first).unwrap();
        let installed =
            install_offline_payload(&temp.path().join("managed"), &payload, &first_result.sha256)
                .unwrap();
        assert_eq!(
            fs::read(
                installed
                    .version_root
                    .join("texmf-dist/tex/plain/alias.tex")
            )
            .unwrap(),
            b"plain data"
        );
        assert!(
            !fs::symlink_metadata(
                installed
                    .version_root
                    .join("texmf-dist/tex/plain/alias.tex")
            )
            .unwrap()
            .file_type()
            .is_symlink()
        );
    }

    #[test]
    fn rejects_escaping_links_and_inventory_mismatch() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let inventory = fixture(&source);
        fs::write(temp.path().join("outside"), b"outside").unwrap();
        symlink(
            temp.path().join("outside"),
            source.join("texmf-dist/tex/plain/escape.tex"),
        )
        .unwrap();
        let mut output = Vec::new();
        assert!(matches!(
            build_payload(
                &source,
                &inventory,
                &PayloadConfig {
                    toolchain_id: "texlive-2026.0-x86_64-linux".to_owned(),
                    texlive_revision: 80315,
                },
                &mut output,
            ),
            Err(PackageError::UnsafeEntry(_))
        ));

        fs::remove_file(source.join("texmf-dist/tex/plain/escape.tex")).unwrap();
        let wrong = serde_json::to_vec(&serde_json::json!({
            "formatVersion": 1,
            "texliveYear": 2026,
            "texliveRevision": 1,
            "platform": "x86_64-linux",
            "tlpdbSha512": "a".repeat(128),
            "roots": REQUIRED_ROOTS,
            "packageCount": 1,
            "packagesWithoutDeclaredLicense": ["fixture"],
            "packages": [{
                "name": "fixture",
                "license": null,
                "dependencies": [],
                "archiveBytes": {"runtime": 1, "documentation": 0, "source": 0},
                "installedBytes": {"runtime": 1, "documentation": 0, "source": 0}
            }]
        }))
        .unwrap();
        assert!(matches!(
            build_payload(
                &source,
                &wrong,
                &PayloadConfig {
                    toolchain_id: "texlive-2026.0-x86_64-linux".to_owned(),
                    texlive_revision: 80315,
                },
                Vec::new(),
            ),
            Err(PackageError::Inventory(_))
        ));
    }
}
