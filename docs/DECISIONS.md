# Architectural decisions

Accepted decisions are recorded in `docs/adr/`:

1. Filesystem source of truth.
2. Typed Rust/TypeScript boundary.
3. Root-contained symlink policy.
4. Managed TeX distribution direction.
5. Explicit build permissions.
6. Non-proprietary build outputs.
7. Tolerant project indexing.
8. External notation profiles.
9. Separate paper library.
10. Reviewed, optional AI.

The managed-distribution direction in item 4 is refined by
[`ADR-002`](adr/002-managed-texlive-2026.md): V0.1 pins a runtime-only curated
TeX Live 2026 payload, installed immutably and updated by atomic version switch.

Use `adr/000-template.md` for changes. A superseding ADR must identify the prior
decision and migration consequences.
