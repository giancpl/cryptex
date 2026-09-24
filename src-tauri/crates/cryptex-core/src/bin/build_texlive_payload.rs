use cryptex_core::toolchain_package::{PayloadConfig, build_payload};
use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};
use tempfile::NamedTempFile;

struct Options {
    source: PathBuf,
    inventory: PathBuf,
    output: PathBuf,
    toolchain_id: String,
    revision: u32,
}

fn parse_arguments() -> Result<Options, String> {
    let mut source = None;
    let mut inventory = None;
    let mut output = None;
    let mut toolchain_id = None;
    let mut revision = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {argument}"))?;
        match argument.as_str() {
            "--source" => source = Some(PathBuf::from(value)),
            "--inventory" => inventory = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--toolchain-id" => toolchain_id = Some(value),
            "--revision" => {
                revision = Some(
                    value
                        .parse()
                        .map_err(|_| "revision must be an integer".to_owned())?,
                )
            }
            _ => return Err(format!("unexpected argument: {argument}")),
        }
    }
    Ok(Options {
        source: source.ok_or_else(usage)?,
        inventory: inventory.ok_or_else(usage)?,
        output: output.ok_or_else(usage)?,
        toolchain_id: toolchain_id.ok_or_else(usage)?,
        revision: revision.ok_or_else(usage)?,
    })
}

fn usage() -> String {
    "usage: build_texlive_payload --source DIR --inventory FILE --output FILE \
     --toolchain-id ID --revision NUMBER"
        .to_owned()
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_arguments().map_err(io::Error::other)?;
    let inventory = fs::read(&options.inventory)?;
    let parent = options
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let source = options.source.canonicalize()?;
    let inventory_path = options.inventory.canonicalize()?;
    let output_parent = parent.canonicalize()?;
    if inventory_path.starts_with(&source) || output_parent.starts_with(&source) {
        return Err(
            io::Error::other("inventory and output must be outside the runtime source").into(),
        );
    }
    let mut temporary = NamedTempFile::new_in(parent)?;
    let result = build_payload(
        &source,
        &inventory,
        &PayloadConfig {
            toolchain_id: options.toolchain_id,
            texlive_revision: options.revision,
        },
        temporary.as_file_mut(),
    )?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(&options.output)
        .map_err(|error| error.error)?;
    let checksum = options.output.with_extension(
        options
            .output
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!("{value}.sha256"))
            .unwrap_or_else(|| "sha256".to_owned()),
    );
    let mut checksum_file = NamedTempFile::new_in(parent)?;
    writeln!(
        checksum_file,
        "{}  {}",
        result.sha256,
        options
            .output
            .file_name()
            .unwrap_or_else(|| OsStr::new("payload"))
            .to_string_lossy()
    )?;
    checksum_file.flush()?;
    checksum_file.as_file().sync_all()?;
    checksum_file
        .persist(&checksum)
        .map_err(|error| error.error)?;
    File::open(parent)?.sync_all()?;
    println!(
        "payload {}: {} entries, {} expanded bytes, SHA-256 {}",
        options.output.display(),
        result.entries,
        result.expanded_bytes,
        result.sha256
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
