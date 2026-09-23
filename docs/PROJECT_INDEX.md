# Project index schema

G1 freezes the version-1 shape consumed by navigation, Command Finder, and notation
features. The index is rebuildable metadata; source files remain canonical and no index
file is written into a project.

## Semantics

- `ProjectIndex.completeness` is always `bestEffort`. An absent record never proves
  that a LaTeX construct or meaning is absent.
- Each file is `complete`, `partial`, or `skipped`. Issues explain truncation,
  malformed input, unsupported encoding, missing includes, and include cycles.
- Records cover sections, labels, references, citations, environments, macro
  definitions/usages, includes, packages, and `cryptocode` constructs.
- Every record preserves an exact source range plus `confidence` and `provenance`.
  `recovered`/`errorRecovery` means the scanner continued after malformed input;
  it does not assert TeX semantics.
- `name` is the observed identifier or construct. Optional `target` is reserved for
  a normalized, deterministically resolved target when one is available.
- Relative paths must pass the same traversal-safe `ProjectPath` rules as filesystem
  operations. Fingerprints are lowercase SHA-256 digests of scanned content.
- `schemaVersion` changes when persisted/index DTO meaning becomes incompatible;
  `apiVersion` follows the backend boundary version.

## Default scanner limits

| Limit              | Default |
| ------------------ | ------: |
| Project files      |  10,000 |
| Total source bytes | 256 MiB |
| Bytes per file     |   5 MiB |
| Records per file   |  50,000 |
| Brace nesting      |     256 |
| Bytes per command  |   4,096 |

G2 must stop or degrade to a partial/skipped result at these limits. It must not expand
macros, execute TeX, infer mathematical equivalence, or claim to parse the complete TeX
language. G3 may add graph resolution but may not weaken these per-scan bounds.

## Per-file scanner

G2 implements a linear lexical pass over UTF-8 source. Comments, inline `\verb`, and
`verbatim`, `verbatim*`, `lstlisting`, and `minted` bodies are excluded. Balanced
arguments are inspected without expanding or executing macros. Unsupported and
ambiguous constructs are omitted; malformed constructs produce partial status and
structured issues while later top-level commands remain discoverable where possible.

## Project graph and updates

G3 indexes `.tex` and `.sty` files in stable path order and resolves lexical include
targets relative to their source file, adding `.tex` only when no extension is present.
It records forward and reverse edges, reports missing targets and cycles, and consumes
the normalized C3 watcher DTO. Modify/create events scan only existing changed sources;
rename/remove events evict vanished paths. A rescan uses a cancellable unpublished
snapshot and increments the generation only when the result is committed.
