# Linux packaging

CrypTex V0.1 initially supports Ubuntu 24.04 on x86_64. The application smoke
artifacts are a Debian package and an AppImage. This is a deliberately narrow first
target; other distributions and architectures remain unverified rather than being
implied by Tauri's portable architecture.

Run `pnpm packaging:validate` to verify that the checked-in Tauri configuration is
explicitly limited to those two formats, uses GPL-3.0-or-later, and remains protected
from accidental npm publication. The manually dispatched `Linux package smoke
test` workflow builds both formats, records SHA-256 checksums, and retains them for
seven days as CI evidence. It has no release permission and does not publish to a
GitHub release or package registry.

This first J5 gate covers only application-bundle construction. A distributable V0.1
still requires all of the following:

- complete application/dependency notices for the approved GPL-3.0-or-later release;
- a frozen TeX Live runtime archive, package inventory, checksums, notices, and
  matching source-availability procedure from ADR-002;
- an audited resolution for Biber's `libcrypt.so.1` dependency;
- release integration for the implemented bounded offline payload extraction and
  atomic activation into application data;
- clean-machine install, offline fixture compilation, upgrade, rollback, uninstall,
  and project-preservation evidence;
- Linux process/filesystem sandbox validation and release signing/update policy.

The workflow artifacts are therefore test inputs, not releases. They must not be
redistributed as CrypTex V0.1.

## TeX Live inventory

`pnpm toolchain:inventory -- --tlpdb FILE --revision 80315 --expected-sha512 HEX
--output FILE` derives the exact sorted dependency closure from the frozen TLPDB.
It verifies the database hash before parsing and emits package dependencies, declared
licenses, archive sizes, installed sizes, roots, platform, revision, and the explicit
list of packages with no declared license. Output is written through an adjacent
temporary file and atomic rename.

The generated inventory is evidence, not legal approval. Every entry without a
declared license still requires manual review, and matching notices/source
availability must accompany the release payload.

## Reproducible runtime payload

`pnpm toolchain:package -- --source DIR --inventory FILE --output FILE.tar.zst
--toolchain-id texlive-2026.0-x86_64-linux --revision 80315` packages a prepared
runtime without consulting the network or ambient TeX installation. Files are sorted,
TAR ownership and timestamps are normalized, permissions are reduced to read/execute,
and Zstandard parameters are fixed. Safe in-root file symlinks are materialized because
the installer accepts no links; escaping, directory, broken, and special-file entries
fail the build.

The builder embeds the verified inventory and a manifest containing hashes of all
required executables, then writes a SHA-256 sidecar. Rebuilding identical input must
produce identical archive bytes. The existing offline installer test consumes the
generated archive, so producer and consumer formats are checked together.

Installed versions can be enumerated and reactivated by validated identifier. Every
rollback repeats manifest, platform, binary hash, and executable probes before the
atomic active-version switch. Old versions are intentionally retained: deletion stays
disabled until the build supervisor can prove that no running operation holds one.
Installation requires at least 1.5 GB available on the exact application-data
filesystem before staging. This provisional ADR-002 threshold must be replaced with
the measured clean-machine peak before V0.1 release.
