# Notation profiles

CrypTex notation profiles are versioned JSON data stored outside LaTeX projects. They
list literal LaTeX spellings that the user has explicitly declared for a concept; they
do not encode or infer mathematical equivalence.

## Version 1

A profile contains `version`, `name`, and a nonempty `concepts` array. Every concept
contains a stable lowercase-hyphenated `id`, a display `label`, one `preferredForm`,
and one or more unique `declaredForms`. The preferred form must occur exactly in the
declared forms. A literal form may belong to only one concept, avoiding ambiguous
scanner evidence.

Bundled defaults currently cover the security parameter, adversary, challenger,
negligible function, and probability. They are ordinary configurable defaults, not
mathematical assertions.

Project overrides contain `version` and concept records with `conceptId`,
`preferredForm`, and `declaredForms`. They may override only concepts in the current
global/default profile. Effective precedence is:

1. project override keyed by canonical project identity;
2. global user profile;
3. bundled CrypTex defaults.

The effective DTO reports `default`, `global`, or `project` provenance for each
concept. Settings live in `notation-profiles.json` in the application configuration
directory and are replaced atomically with restrictive directory permissions. No
file is added to a project.

## Import and export

Import accepts bounded JSON content and validates it before replacing the global
profile. Unknown fields, unsupported versions, unknown override concepts, duplicate
identifiers/forms, empty or oversized values, and ambiguous forms are rejected
explicitly. Export returns the active global profile, or the defaults when no global
profile is configured. UI file selection is deferred to I2 and must pass content—not
a filesystem path—across the backend boundary.

## Palette and insertion

The I2 palette searches the effective profile by concept ID, label, preferred form,
and declared exact forms. It shows whether each preference comes from the bundled
default, global profile, or project override. Selecting **Insert preferred form**
replaces the current editor selection with the exact preferred string in one undoable
CodeMirror transaction. Palette insertion does not interpret snippet placeholders and
does not modify profiles, preambles, packages, or other files.

## High-confidence usage scanning

I3 recognizes only exact declared forms that begin with a LaTeX command. Plain symbols
and identifiers are intentionally omitted because their meaning is ambiguous without
mathematical interpretation. A command-boundary check prevents a short form such as
`\Pr` from matching `\Prime`. Comments, escaped command starts, inline `\verb`/
`\verb*`, and `verbatim`, `verbatim*`, `lstlisting`, and `minted` environments are
skipped.

Results include concept ID, matched form, preferred status, source range, and disk
fingerprint. The project scan uses the current index limits and marks itself incomplete
when input is skipped, truncated, unreadable, or changed since indexing. It never turns
uncertain text into a usage.

## Consistency findings and suppressions

I4 emits an informational finding when an exact declared form differs from the
configured preference. If multiple declared forms for the same concept occur, each
nonpreferred exact occurrence becomes a warning and lists the deterministic evidence.
Preferred-only usage produces no finding. Messages describe consistency with a user
profile and never say that notation is mathematically wrong.

The Notation checks panel navigates to the exact source range and reports when the
underlying scan is incomplete. A user can suppress a concept project-wide through the
typed API or suppress it in one file from the panel. Suppressions contain only a known
concept ID and optional validated project-relative path; they are stored outside the
project and never modify LaTeX. I4 offers no automatic replacement.
