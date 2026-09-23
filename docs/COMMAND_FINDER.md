# Command Finder

Command Finder is a deterministic offline catalog of standard LaTeX and real
`cryptocode` commands. Entries include concepts, synonyms, package/version
requirements, signature, snippets, examples, contexts, documentation provenance,
and deterministic ranking metadata.

Insertion is explicit and undoable. The Finder warns about missing package
requirements but does not silently edit the preamble or install packages. No LLM
or network catalog is part of V0.1.

## Deterministic ranking

H3 searches locally across literal commands, stable IDs, display names, concepts,
synonyms, signatures, and summaries. Exact matches outrank prefixes, which outrank
fuzzy matches with at most three edits. Fixed boosts favor the current editor context
and entries whose required package appears in the best-effort project index. Missing
requirements do not hide a result; H4 can therefore display the required warning.

The order is total and reproducible: score descending, match quality, then stable entry
ID. Queries are limited to 256 UTF-8 bytes, results to 100, and package names are
validated. The regression budget runs 1,000 searches of the bundled catalog in under
five seconds in an unoptimized test build; interactive callers request only the first
small result page.

## Palette and insertion

H4 opens Command Finder from the editor toolbar or `Ctrl/Cmd+K`. Search, context
selection, result traversal, insertion, and dismissal are keyboard operable. Result
details show the exact signature, summary, provenance, and any package requirement
that the project index does not currently contain.

Insertion is always explicit. TextMate-style placeholders are expanded in the active
CodeMirror buffer; an existing selection replaces the first placeholder default, and
the first field remains selected. The replacement is dispatched as one editor
transaction and therefore one undo step. CrypTex neither edits `\usepackage` lines
nor installs packages on the user’s behalf.
