# ADR-001: Foundational product and architecture decisions

- Status: accepted
- Date: 2026-09-19

## Context

CrypTex must provide local assistance without replacing standard LaTeX or making
internal application state authoritative.

## Decision

- Filesystem project files are canonical.
- Rust owns privileged operations behind a typed command/event boundary.
- Symlinks may be shown, but file operations require their resolved target to stay
  inside the canonical project root.
- Compilation uses a managed, versioned TeX distribution; D1 selects its contents.
- `.latexmkrc` and shell escape require separate explicit permissions.
- CrypTex-specific state and build caches stay outside project folders.
- Project indexing is tolerant and best-effort, never a claim of full TeX semantics.
- Notation profiles are external and checks recognize declared exact forms only.
- The PDF library is independent from project and `.bib` ownership.
- AI is optional, disabled by default, and may propose but never silently apply edits.

## Consequences

Conflict handling, secure path revalidation, managed-toolchain licensing, typed DTO
maintenance, and explicit review workflows are required work rather than optional
polish.

## Validation

Tasks A4, B1-B3, D1-D4, G1-G3, I1-I5, K1-K4, and L1-L5 validate these decisions.
