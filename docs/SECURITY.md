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

Restricted TeX is not a proven sandbox. OS sandboxing is a release investigation,
not a reason to weaken the controls above.
