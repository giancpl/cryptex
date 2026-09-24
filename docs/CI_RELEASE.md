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
