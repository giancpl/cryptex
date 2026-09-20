# ADR-002: Managed TeX Live 2026 distribution

- Status: accepted for D2 implementation
- Date: 2026-09-20

## Context

CrypTex V0.1 must compile ordinary projects with `latexmk`, pdfLaTeX,
XeLaTeX, LuaLaTeX, BibTeX, Biber, TikZ, and the real `cryptocode` package.
Compilation must work without a system TeX installation and, once installed,
without network access. Project-controlled executables, `.latexmkrc`, and shell
escape remain disabled unless a later trust decision explicitly permits the last
two.

TeX Live 2026 was released on 2026-03-01. Its net installer supports unattended
profiles and collection selection, while `tlmgr` supports package installation,
updates, backups, and restores. The full ISO is stable but approximately 6 GB and
is too large for the default V0.1 payload.

Primary references:

- [TeX Live 2026 guide](https://tug.org/texlive/doc/texlive-en/texlive-en.html)
- [`install-tl` profiles](https://tug.org/texlive/doc/install-tl.html#PROFILES)
- [`tlmgr` operations and verification](https://tug.org/texlive/doc/tlmgr.html)
- [TeX Live copying and redistribution](https://www.tug.org/texlive/copying.html)
- [TLPDB size units](https://www.tug.org/texlive/doc/tlpkgdoc/TLPOBJ.html)
- [`cryptocode` 0.44 on CTAN](https://ctan.org/pkg/cryptocode)
- [`latexmk` 4.88 on CTAN](https://ctan.org/pkg/latexmk)

## Options

### System TeX only

Small application downloads, but versions and package coverage differ by Linux
distribution. This contradicts zero-configuration and reproducible fixture builds.

### Full TeX Live ISO

Broadest compatibility and a stable offline source. The official image is around
6 GB, and a runtime-only x86_64 closure is still several gigabytes. This is too
large as the default payload, though it remains a useful release-audit baseline.

### Tectonic or another engine distribution

Potentially smaller and easier to fetch lazily, but it does not preserve the
required `latexmk`-driven workflow and would create a material compatibility gap
with ordinary TeX Live and Overleaf projects.

### Curated TeX Live collections

Uses upstream binaries and package semantics while bounding the payload. A
collection-level baseline is less size-efficient than an individual-package lock,
but it provides a stable compatibility envelope that can be measured and audited.

## Spike evidence

The repository script `scripts/measure-texlive.mjs` computes dependency closure
from an official `texlive.tlpdb`. It resolves `.ARCH` dependencies, rejects missing
packages, separates runtime/documentation/source containers, and uses the official
4 KiB installed-size block unit. Its parser and arithmetic have automated tests.

Measurement source on 2026-09-20:

- repository: `https://mirror.ctan.org/systems/texlive/tlnet`
- TeX Live release/revision: `2026` / `80315`
- platform: `x86_64-linux`
- TLPDB SHA-512:
  `6c30b6ce3373c0930ee37d88d7ebbf746d3b34f921745f87437bd34b4930f97ad473f98a3a568474ca506e5d8996a5aec5a6f365e903e367fa2d910cb1d600b1`
- verified installer size: 5,262,389 bytes
- verified installer SHA-512:
  `3bfd0911d8dda1a934350e97a5e5c247873a785a83f151d70a365552155445c012e10f2a8c41f9e6b63e72c4de79e0d71950200f29949d4e5716e4cb22ef0a5a`

The selected roots are checked in at `toolchain/texlive-2026-roots.txt`. The
closure exposes many single and combined license identifiers; 108 selected records
have no `catalogue-license` field, so automated metadata is explicitly insufficient
for release approval.

| Closure               |       Packages | Compressed runtime | Installed runtime | Docs compressed | Sources compressed |
| --------------------- | -------------: | -----------------: | ----------------: | --------------: | -----------------: |
| CrypTex curated roots |          2,694 |      356,094,728 B |     948,514,816 B | 1,568,246,684 B |       50,979,760 B |
| TeX Live full scheme  | 5,189 resolved |    1,684,140,340 B |   4,736,679,936 B | 3,729,532,928 B |      117,493,940 B |

The full-scheme calculation reported `texworks.x86_64-linux` unavailable in the
network repository and is therefore comparative only. The curated closure had no
missing dependency. Values are metadata measurements, not filesystem allocation;
archives, generated formats, caches, and block allocation make actual disk usage
different. D2 must enforce a headroom check of at least 1.5 GB for the runtime and
staging area, then replace that provisional threshold with measured peak usage.

The spike installs the curated roots into an isolated temporary prefix without
documentation or source files and runs `scripts/verify-texlive-spike.sh`. The
fixture set exercises multipass pdfLaTeX, `cryptocode`, TikZ, Biber, XeLaTeX,
LuaLaTeX, and SyncTeX generation. It always passes `-norc`, so a fixture cannot
activate a project `.latexmkrc`. The resulting prefix occupied 951,520,292 bytes.
Downloading 2,685 collection dependencies from the selected CTAN mirror took
18m34s before format generation, which rules out package-by-package installation
as the normal first-run experience.

The first fixture run also found two missing runtime assumptions. The `synctex`
command is a separate package, now an explicit selected root. Biber 2.22 requires
`libcrypt.so.1`; it was absent from the Freedesktop SDK 25.08 even though TeX Live
itself installed correctly. The Biber fixture passed with Debian bookworm
`libcrypt1` 4.4.33-2 (SHA-256
`f5f60a5cdfd4e4eaa9438ade5078a57741a7a78d659fcb0c701204f523e8bd29`) extracted
only into the temporary runtime. D2 must report missing native libraries as a
corrupt/incompatible toolchain, and J5 must either bundle an audited compatible
`libcrypt.so.1` in the sandbox runtime or declare and verify it as a supported-target
package dependency.

## Decision

V0.1 uses a CrypTex-managed, immutable TeX Live 2026 runtime. The baseline roots
are the checked-in list above. Production artifacts are built from a frozen local
TLPDB/repository snapshot, not from mutable `mirror.ctan.org` during application
startup.

The release pipeline will produce two separately checksummed artifacts:

1. the CrypTex application package;
2. a platform-specific, runtime-only TeX payload plus its package manifest,
   licenses, upstream revision, and archive checksums.

An offline release bundle contains both. First run only verifies and extracts the
payload into a versioned application-data directory. It performs no network
installation. A future explicit repair/download flow may fetch the same signed
payload, but compilation never downloads packages on demand.

D2 discovers only an exact manifest-approved executable beneath the active
versioned root. It records TeX Live revision and executable versions in build
metadata. It does not search the project or silently fall back to `PATH`.

Updates install into a new sibling version, verify all hashes and the package
manifest, run the complete fixture suite, then atomically switch the active-version
record. The previous known-good version is retained for rollback. A running build
holds its resolved toolchain version, so activation cannot mutate it mid-build.
Rollback switches versions; it does not attempt in-place `tlmgr restore`.

`tlmgr` is a release-engineering input only. It is never exposed to projects or
used by normal compilation. Annual TeX Live upgrades are separate toolchain
versions. Within-year refreshes require regenerated locks, license inventory, and
fixture evidence.

## Licensing and notices

TeX Live states that its included material is redistributable, commonly with a
requirement that source remain available, but each package's license is final.
Therefore J5 must generate an inventory from the frozen TLPDB, retain upstream
license/readme material in a notices artifact, publish or offer the matching source
payload as required, and audit native binary dependencies. Removing documentation
and sources from the installed runtime is a size decision, not a waiver of source
or notice obligations. `latexmk` is GPL-2.0, `cryptocode` is LPPL-1.3, and Biber is
Artistic-2.0 in the measured snapshot.

No release is permitted from a mutable mirror without a frozen package manifest,
per-container checksums, a complete license inventory, and documented source
availability. Legal review remains a J5 release gate.

## Security consequences

- The managed prefix is read-only during compilation and never writable by a
  project process.
- D3 invokes the manifest-approved `latexmk` directly with argument arrays and a
  sanitized environment; the app never invokes a shell.
- Shell escape and project `.latexmkrc` remain independently disabled by default.
- A broad package set improves compatibility but is not a sandbox. D3/J5 must test
  process and filesystem isolation on supported Linux packages.
- Package downloads, updates, and activation are separate privileged workflows and
  cannot occur as a side effect of opening or compiling a project.
- Readiness checks execute version probes for `latexmk`, every engine, BibTeX,
  Biber, and SyncTeX so missing dynamic libraries fail before a build is queued.
- Logs, child processes, CPU time, memory, and output files remain bounded even
  with the managed distribution.

## Consequences

The selected runtime is approximately 950 MB before generated formats and staging,
which is substantial but far below the full distribution. Users gain deterministic
engines and broad common-package coverage. Projects outside that envelope receive
an actionable missing-package diagnostic; V0.1 does not silently fetch packages or
execute arbitrary install hooks.

Documentation is not included in the runtime payload. CrypTex's command catalog
may link to upstream documentation, while the release keeps notices and required
source availability separately. The package roots may only change through an ADR
amendment with new measurements and fixture results.

## Validation and remaining gates

D1 is complete when the checked-in measurement test and the isolated fixture
runner pass. The current development environment cannot prove a clean-VM install,
Linux package integration, filesystem sandboxing, or a physically disconnected
first run; those remain explicit J5 gates rather than inferred success.

D2 must implement signed/checksummed manifest discovery, readiness/repair states,
version pinning, and atomic activation. D3 must implement process supervision. J1
expands the fixtures, and J5 repeats installation and compilation on every supported
clean Linux target with networking disabled.
