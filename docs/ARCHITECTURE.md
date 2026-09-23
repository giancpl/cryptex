# Architecture

The React/TypeScript frontend owns presentation and transient view state. The
Tauri/Rust backend owns project identity, validated paths, filesystem operations,
watching, processes, compilation, indexing, and durable local settings.

The boundary consists of versioned serializable DTOs, typed commands, typed
events, stable error codes, and operation IDs. Frontend code never receives a
general filesystem or process primitive.

Project source folders contain only standard project files. CrypTex settings,
trust records, recovery snapshots, caches, and rebuildable indices live in the
platform application-data/cache locations keyed by canonical project identity.

See `DECISIONS.md` and `adr/` for binding architectural decisions.

## Managed toolchain boundary

D2 resolves TeX only from `app_data/toolchains/versions/<toolchain-id>` through
a strict versioned manifest and `active.json`. Every required executable is
canonicalized beneath that root, SHA-256 checked, and version-probed with a cleared
environment before the toolchain can be reported ready. Project paths and ambient
`PATH` are never discovery inputs. See ADR-002.

## Restricted process boundary

D3 keeps process creation inside the Rust core; there is intentionally no generic
frontend execution command. Callers select a named, pre-registered executable and
provide an argument array plus a working directory that is canonicalized beneath an
approved root immediately before spawning. The supervisor clears the inherited
environment, supplies only the managed binary directory and stable locale/timezone,
streams a bounded combined stdout/stderr budget, and enforces timeout, cancellation,
and resource limits. On Linux, each child starts in a fresh process group so
termination covers descendants.

## Project build permissions

D4 stores build permissions in `app_config/project-trust.json`, keyed by the
canonical project identity. Project `.latexmkrc` execution and TeX shell escape are
independent, denied by default, described explicitly through typed DTOs, and
revocable together or separately. Commands accept only currently open project
identities; build construction must query the backend trust service and cannot rely
on frontend state.

## Build configuration resolution

E1 resolves a build to one validated project-relative root and one closed engine enum:
`pdfLatex`, `xeLatex`, or `luaLatex`. Explicit project preferences stored outside
the source tree take precedence; otherwise a supported TeX magic comment is used,
then pdfLaTeX is the default. Both decisions carry provenance. Ambiguous roots,
unsupported engines, and magic values containing flags or command fragments fail
before process request construction.

## Latexmk request construction

E2 produces an internal, inspectable request for the named managed `latexmk`
executable; it never accepts an executable or free-form command from the frontend.
The builder uses argument arrays, sends all generated files to a canonical per-project
app-cache directory, requests SyncTeX and recorder output, and records the expected
PDF, SyncTeX, log, recorder, and latexmk database paths. Automatic rc discovery is
always disabled with `-norc`; an in-root `.latexmkrc` is loaded explicitly with
`-r` only when backend trust allows it. Later fixed arguments select the resolved
engine and explicit shell-escape mode.

## Build scheduling

E3 models scheduling as a backend state machine independent of Tauri and worker
threads. Each project has at most one active and one coalesced queued request. New
save builds replace older queued saves; an explicit build preempts an active save
and suppresses its eventual result; saves arriving behind a queued explicit build
coalesce into it. Monotonic operation IDs scope cancellation and completion, and a
completion for any non-active ID is stale by definition. Different projects remain
independent and may execute concurrently.

E4 resolves and re-verifies only the managed `latexmk` executable before execution,
then connects scheduled requests to the restricted supervisor. Typed build-state and
bounded output events drive the UI without exposing process primitives. Successful
artifacts remain in the canonical per-project app-cache directory; failed builds do
not erase last-success metadata, and cleanup can remove only the exact validated cache
directory while no build is active.

## Diagnostic extraction

F1 reads at most 4 MiB from the generated LaTeX log and tolerates malformed or
truncated input. It extracts stable diagnostic codes for located TeX errors, classic
errors, warnings, over/underfull boxes, undefined references and citations,
bibliography warnings, and latexmk failures. A source range is emitted only when its
path canonicalizes to a regular file beneath the project root; uncertain locations
remain unlocated. The latest published operation may expose its bounded raw log
through an operation-scoped command that revalidates the artifact inside app cache.

## Diagnostic presentation

F2 keeps diagnostic rendering in React and CodeMirror while source validation remains
in Rust. Terminal build diagnostics can be filtered by severity; located entries open
the validated project-relative file, select the reported line, and add a non-mutating
line decoration. Unlocated entries remain visible but cannot navigate. A monotonic
frontend project-edit epoch labels results stale and removes inline markers after any
subsequent edit, while the operation-scoped raw log is fetched only on explicit user
request.

## PDF artifact and viewer boundary

F3 retains only the latest successful PDF capability per project and operation. The
frontend cannot submit a path: it requests bytes using those opaque identities, and
Rust revalidates the exact regular cache artifact before a bounded 128 MiB binary IPC
response. PDF.js 6.3.289 and its worker are bundled locally. The custom canvas viewer
does not instantiate PDF.js annotation or scripting layers, disables XFA and worker
fetches, caps decoded images and canvas pixels, limits text search to 500 pages, and
persists only page/zoom view state outside project files.

## Last-successful PDF publication

F4 copies a successful build output into an operation-named immutable snapshot using
a same-directory temporary file, flush, and atomic rename before publishing it. The
registry switches only after the snapshot is complete; a failed, cancelled, timed-out,
stale, oversized, or unpublishable build leaves the prior capability unchanged. Reads
hold the registry guard while consuming the bounded snapshot, so replacement cannot
retire it midway. The frontend adds a monotonic request token: responses from older
builds or projects cannot replace the current preview. A new valid document refreshes
the existing viewer instance, preserving page and zoom where possible.

## Forward SyncTeX

F5 atomically retains the successful `.synctex.gz` beside its operation-specific PDF.
A typed request contains only an open project identity, retained operation identity,
validated project-relative source path, and bounded line/column. Rust re-verifies the
managed `synctex` executable and both retained artifacts, then invokes `synctex view`
through the restricted process supervisor with a ten-second timeout and bounded output.
The tolerant parser returns the first finite page-space `h`, `v`, `W`, and `H` result
as a normalized DTO. React suppresses stale requests; PDF.js changes page, preserves
zoom, scrolls toward the result, and renders a temporary noninteractive marker. Missing
data or unmatched lines degrade to a visible unavailable state.

## Inverse SyncTeX

F6 converts preview clicks to PDF coordinates and invokes `synctex edit` for the
currently retained build. Rust bounds and parses the result, canonicalizes the reported
source immediately before use, and requires a regular file contained by the canonical
project root. Only then does React receive a project-relative path and line/column; stale
responses are discarded before navigation.
