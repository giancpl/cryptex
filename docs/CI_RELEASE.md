# CI and release

Pull requests run formatting checks, Clippy, Rust tests, TypeScript checks, lint,
frontend tests, production web build, binding-drift checks, and dependency/license
audits. Fixture and packaging matrices are added as their milestones land.

Releases require clean supported Linux VM installation, offline compilation,
upgrade/uninstall tests, security regression tests, published supported targets,
license inventory, and reproducible artifacts or documented nondeterminism.
