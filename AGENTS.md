# Contributor instructions

Read `README.md`, `docs/PRODUCT.md`, `docs/ARCHITECTURE.md`,
`docs/SECURITY.md`, `docs/DECISIONS.md`, and `docs/TASKS.md` before editing.

- Keep standard LaTeX and the filesystem canonical.
- Privileged filesystem and process operations belong in Rust.
- Never accept unchecked absolute paths from the frontend.
- Never invoke LaTeX through a shell or silently enable `.latexmkrc` or shell escape.
- Add tests with behavior changes and run `pnpm check` and `pnpm test`.
- Amend an ADR before changing an architectural decision.
- Do not implement post-V0.1 scope while an M0-M7 prerequisite is incomplete.
