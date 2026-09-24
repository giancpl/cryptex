# CI and release

Pull requests run formatting checks, Clippy, Rust tests, TypeScript checks, lint,
frontend tests, production web build, binding-drift checks, and dependency/license
audits. Fixture and packaging matrices are added as their milestones land.

Releases require clean supported Linux VM installation, offline compilation,
upgrade/uninstall tests, security regression tests, published supported targets,
license inventory, and reproducible artifacts or documented nondeterminism.

The manual Linux package smoke workflow targets Ubuntu 24.04 x86_64 and produces
short-lived Debian/AppImage test artifacts plus SHA-256 checksums. It intentionally
has read-only repository permissions and no publishing step. See `PACKAGING.md`.

## Fail-closed release evidence

`pnpm release:validate -- --evidence /path/to/evidence/release.json` is the final
J6 machine-readable gate. Paths in the JSON are relative to its directory and cannot
escape it. The gate recomputes SHA-256 digests for the Debian bundle, AppImage,
TeX Live payload, inventory, and notices, and requires file-backed passing evidence
for clean installation, offline compilation, upgrade, rollback, project-preserving
uninstall, adversarial and acceptance suites, portability, license and sandbox
review, signing, and reproducibility.

Set `CRYPTEX_RELEASE_COMMIT` to the expected 40-character commit SHA when invoking
the gate in the release environment. Missing, stale, duplicated, unknown, absolute,
escaping, or tampered evidence fails validation. Passing this command does not create
or publish a release; it proves that one evidence directory is internally complete.
