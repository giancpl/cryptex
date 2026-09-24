# Licensing

CrypTex application source is licensed under GPL-3.0-or-later. The repository root
`LICENSE` contains the complete GPL version 3 text; recipients may use that version
or, at their option, any later version. This decision covers code authored for
CrypTex, not third-party components or user projects.

Dependency metadata and notices must be generated in CI. TeX Live package licenses,
redistribution terms, source obligations, font licenses, PDF.js, Tauri, and any
packaged native libraries still require their own review during J5.

`pnpm licenses:generate` regenerates `licenses/dependencies.json` and
`THIRD_PARTY_NOTICES.md` from the locked production npm graph and locked Cargo
registry graph. CI runs `pnpm licenses:check` and rejects drift. The generated report
is an inventory; release packaging must still include required license texts and the
separate TeX Live/native-library notices.

No managed TeX artifact may be published until its package manifest, licenses,
notices, checksums, and update provenance are recorded.
