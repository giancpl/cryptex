# Linux packaging

CrypTex V0.1 initially supports Ubuntu 24.04 on x86_64. The application smoke
artifacts are a Debian package and an AppImage. This is a deliberately narrow first
target; other distributions and architectures remain unverified rather than being
implied by Tauri's portable architecture.

Run `pnpm packaging:validate` to verify that the checked-in Tauri configuration is
explicitly limited to those two formats and that the workspace remains private while
the application license is unresolved. The manually dispatched `Linux package smoke
test` workflow builds both formats, records SHA-256 checksums, and retains them for
seven days as CI evidence. It has no release permission and does not publish to a
GitHub release or package registry.

This first J5 gate covers only application-bundle construction. A distributable V0.1
still requires all of the following:

- an approved application license and complete application/dependency notices;
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
