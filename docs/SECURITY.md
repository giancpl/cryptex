# Security

LaTeX projects are untrusted input and may attempt filesystem or process access.

- Canonicalize project roots and revalidate every sensitive relative path in Rust.
- Refuse traversal and symlink resolution outside the root.
- Use fixed executables and argument arrays; never invoke a shell.
- Sanitize process environments, bound output/time/resources, and terminate trees.
- Ignore project `.latexmkrc` and disable shell escape by default. Each requires a
  separate, explicit, revocable project permission.
- Use fingerprints and atomic replacement to avoid silent overwrites.
- Bound file, parser, log, PDF, and indexing input.
- Serve build artifacts through scoped capabilities, not arbitrary file URLs.

The D3 supervisor implements the fixed-executable, argument-array, cleared-environment,
bounded-output, timeout/cancellation, resource-limit, and process-group controls. It
is an internal backend service and is not exposed as a general process command.
Project `.latexmkrc` and shell escape permissions are stored independently, denied
by default, and checked from backend state keyed by canonical project identity. The
frontend can request or revoke permission but cannot assert trusted build flags.
Build engines cross the backend boundary as a closed enum; TeX magic comments cannot
supply executable paths, flags, shell syntax, or arbitrary engine names. The E2
request builder always names managed `latexmk`, suppresses automatic rc discovery,
validates any explicitly permitted project rc inside the root, prefixes root filenames
so they cannot become options, and derives shell-escape flags only from backend trust.
E3 cancellation targets the active operation token, and a preempted or mismatched
operation cannot publish a result or replace newer build state. F1 bounds log reads,
rejects source locations outside the canonical project root, and serves only the
retained operation log after revalidating it as a regular in-cache artifact. F3 applies
the same operation-scoped cache validation to PDFs, transfers bounded binary data
instead of paths, and uses a local canvas-only PDF.js integration without annotation,
form, XFA, or scripting layers. F4 publishes successful PDFs as atomic immutable
operation snapshots; stale IPC responses are rejected by a monotonic frontend token,
and replacement/removal is serialized with bounded reads. F5 accepts only a validated
in-project source path and the currently retained operation, executes only the verified
managed `synctex` binary through the restricted supervisor, bounds time/output, and
rejects non-finite or malformed coordinates before they reach the viewer. F6 also canonicalizes inverse results immediately before use and rejects any source
that is not a regular file contained by the project root, including escaping symlinks.
G1 makes index limits part of the typed schema and applies the existing traversal-safe
project-relative path contract to indexed files and issues. G2 rejects invalid scanner
inputs and enforces file, record, nesting, and command bounds while performing no TeX
execution or macro expansion. G3 canonicalizes discovered files beneath the project
root, tracks canonical directories to stop symlink loops, and publishes full rescans
only after bounded cancellable work completes. G4 exposes only index DTO snapshots;
navigation still rereads a backend-validated project-relative file rather than trusting
a frontend or index path. H1-H2 treat catalog data as inert bundled content, bound its size, validate snippet structure and package identifiers, require HTTPS provenance, and fail loading if bundled data is invalid. Representative snippets compile only inside the existing restricted fixture harness; runtime catalog loading performs no execution or automatic package editing. H3 bounds query bytes and result counts, validates package names, and uses only inert catalog/index strings; fuzzy matching has a maximum edit distance and cannot trigger filesystem, network, or process work. H4 derives package context from the backend-owned index rather than accepting package claims from the UI; insertion affects only the active editor buffer in one user-triggered transaction and never mutates the preamble automatically. H5 uses the same bounded backend search and renders catalog fields with DOM text nodes; it performs no HTML injection, network lookup, or implicit edit. Suggestions are suppressed in confidently detected comments and verbatim regions, and stale asynchronous signature results cannot replace current context. I1 bounds and strictly validates imported notation JSON, rejects unknown fields, versions, concepts, duplicate or ambiguous exact forms, and persists settings atomically outside project roots. Project overrides are accepted only for an open canonical project identity; import/export accepts content rather than arbitrary paths. I2 displays only the backend-validated effective profile and inserts its preferred form only after explicit user action. Notation is inserted as literal text in one editor transaction; it cannot invoke snippet expansion, filesystem access, process execution, or network activity. I3 reads only files already admitted by the backend-owned project index, through validated ProjectService paths, and requires current/indexed fingerprints to match. File bytes, total project bytes, and records are bounded by G1 limits. The scanner performs no macro expansion or TeX execution and omits ambiguous plain forms, comments, escaped commands, inline verbatim, and known verbatim environments. I4 derives findings only from that exact evidence and never executes or rewrites source. Suppressions accept only known concept IDs and validated project-relative paths, are deduplicated and bounded, and remain in atomically persisted user configuration rather than project files.

Restricted TeX is not a proven sandbox. OS sandboxing is a release investigation,
not a reason to weaken the controls above.
