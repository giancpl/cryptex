# Ordered tasks

## Current status

- A1-A5: implemented; desktop compilation is verified by CI because the local
  Flatpak SDK does not provide WebKitGTK development headers.
- B1: implemented with canonical project identity, validated relative paths,
  root-contained symlink policy, typed commands, and native folder selection.
- B2: implemented with bounded lazy directory loading, stable ordering, explicit
  hidden/build-file filters, and inaccessible-entry reporting.
- B3: implemented with bounded UTF-8 reads, binary detection, SHA-256 content
  fingerprints, stale-write rejection, atomic same-directory replacement, flush,
  and permission preservation.
- C1: implemented with CodeMirror 6, LaTeX mode, document tabs, retained per-tab
  undo state, dirty indicators, keyboard editing, and dirty-close confirmation.
- C2: implemented with explicit save, Mod-S, 750 ms autosave, monotone buffer
  revisions, per-file serialized write queues, retryable error state, and dirty
  close/project-switch protection.
- C3: implemented with recursive native watching, 100 ms event coalescing,
  normalized create/modify/remove/rename/rescan events, tree refresh, and
  fingerprint-based self-write correlation.
- C4: implemented with automatic reload for clean buffers, conflict blocking for
  dirty buffers, side-by-side comparison, explicit disk reload, and overwrite
  only after refreshing the disk fingerprint.
- C5: implemented with bounded deterministic root detection, magic-comment and
  document-class evidence, explicit multi-root selection, and atomic per-project
  preferences stored in the user configuration directory.
- C6: implemented with versioned atomic recovery snapshots outside projects,
  restart discovery, corrupt-snapshot warnings, explicit side-by-side review,
  restore, and cleanup after confirmed save or discard.
- D1: completed with the TeX Live 2026 curated-root decision, reproducible TLPDB
  measurement, verified installer identity, isolated multi-engine fixture runner,
  offline/versioned delivery design, update/rollback strategy, and licensing gates.
- D2: implemented with an app-data-only versioned toolchain root, strict manifest
  parsing, canonical-root confinement, SHA-256 executable verification, native
  version probes, typed readiness states, and no project/PATH fallback.
- D3: implemented as an internal Rust-only supervisor with named allowlisted
  executables, argument arrays, canonical working-root enforcement, a cleared
  environment, bounded output streaming, timeout/cancellation, Linux resource
  limits, and whole-process-group termination. No generic execution command is
  exposed to the frontend.
- D4: implemented with per-canonical-project trust records outside source trees,
  separate denied-by-default permissions for project `.latexmkrc` and shell escape,
  exact consequence text in the typed API, revocation, restrictive file permissions,
  and backend authorization checks usable by build request construction.
- E1: implemented with deterministic C5 root reuse, explicit ambiguity errors,
  pdfLaTeX/XeLaTeX/LuaLaTeX as a closed typed engine set, safe TeX magic-comment
  parsing, externally stored project engine preferences, and provenance for both
  effective root and engine. Unsupported values and command fragments are rejected.
- E2: implemented as a pure, inspectable request builder targeting only the named
  managed `latexmk` executable. It fixes engine, nonstop/error, SyncTeX, recorder,
  bibliography, output-directory, and explicit shell-escape flags; prefixes root
  filenames to prevent option injection; keeps artifacts in a validated app-cache
  directory; and loads an in-root `.latexmkrc` only with backend permission while
  suppressing every automatic rc file.
- E3: implemented as a deterministic generic scheduler with one active build per
  project, monotonic operation IDs, save-request coalescing, explicit-build priority,
  cancellation tokens, independent parallel projects, and stale-result suppression.
- E4: implemented with verified managed-executable resolution, scheduler-to-supervisor
  wiring, bounded typed output events, visible queued/running/terminal states, explicit
  cancellation, last-success metadata, and cache-confined cleanup.
- Next: F1 structured LaTeX log parsing.

The authoritative sequence is:

1. A1 specifications and ADRs.
2. A2 Tauri/React/TypeScript bootstrap.
3. A3 checks and tests.
4. A4 typed backend boundary.
5. A5 CI and dependency controls.
6. D1 managed TeX packaging spike.
7. B1 safe project identity and paths.
8. B2 bounded file tree.
9. B3 fingerprinted reads and atomic writes.
10. C1 CodeMirror and document tabs.

Continue with F1-F6, G1-G4, H1-H5, I1-I5, and J1-J6 in
dependency order. M8-M10 remain blocked until J6. Each task is an independently
reviewable change with tests and the global acceptance rules.
