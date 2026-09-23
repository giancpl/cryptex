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
rejects non-finite or malformed coordinates before they reach the viewer.

Restricted TeX is not a proven sandbox. OS sandboxing is a release investigation,
not a reason to weaken the controls above.
