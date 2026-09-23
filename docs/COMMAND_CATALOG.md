# Command catalog

H1 defines the version-1 offline catalog consumed by Command Finder, completion, hover,
and signature help. Catalog data is deterministic, bundled with CrypTex, and never
executes commands or edits project preambles.

## Entry requirements

Every entry must provide:

- a stable lowercase `id`;
- the literal LaTeX `command`, display name, concise summary, and signature;
- at least one normalized concept plus optional synonyms;
- one or more explicit contexts, or `any` by itself;
- package and supported-version requirements where applicable;
- a snippet, optional reviewed examples, and an HTTPS documentation link;
- provenance naming the source, HTTPS source URL, and source/package version.

Snippet placeholders use the TextMate subset `${N:default}` for numbered editable
fields and `$0` for the final cursor. Placeholder numbers must be positive and unique;
literal math dollar signs remain valid.

## Contribution and review

1. Use primary package documentation as provenance. H2 `cryptocode` entries must match
   the package version selected by the managed TeX payload.
2. Do not infer aliases, semantics, package support, or examples that the cited source
   cannot verify.
3. Keep IDs stable after release. A semantic replacement receives a new ID and a
   catalog-version change.
4. Add concepts and synonyms as lowercase human search phrases; do not duplicate them
   case-insensitively within an entry.
5. Compile representative snippets in fixture projects before merging catalog data.
6. Run the Rust schema tests and generated-binding checks. Duplicate IDs, duplicate
   command/signature/context variants, insecure URLs, malformed placeholders, invalid
   package names, and incomplete provenance fail validation.

The schema limits a catalog to 10,000 entries and individual string fields to 16 KiB.
H1 contains no curated command claims; those begin in H2.

## Bundled baseline

H2 ships catalog version `2026.1` as `command-catalog-v1.json`. It includes eight
LaTeX entries for structure, cross-references, citations, and text styling, plus ten
entries from `cryptocode` 0.44 for pseudocode/procedures, assignment and sampling,
control flow, and protocol messages. The dataset is deliberately a useful baseline,
not an exhaustive index.

The LaTeX entries link to the June 2026 reference manual and cite the LaTeX Project core documentation for the LaTeX2e 2026-06-01 release. Every `cryptocode`
entry requires exactly package version 0.44 and cites the official CTAN manual. The
isolated TeX Live fixture `catalog-snippets.tex` compiles representative insertions;
schema tests separately ensure that every bundled entry loads and validates.
