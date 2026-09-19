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

Restricted TeX is not a proven sandbox. OS sandboxing is a release investigation,
not a reason to weaken the controls above.
