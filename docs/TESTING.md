# Testing

- Unit tests cover deterministic utilities, parsers, reducers, schedulers, and ranking.
- Rust integration tests cover paths, symlinks, atomic writes, watchers, processes,
  compiler construction, diagnostics, and SyncTeX.
- Frontend tests cover editor state, panels, Finder, notation, and accessibility.
- Contract tests cover serialized DTOs and generated TypeScript bindings.
- Real LaTeX fixtures cover engines, bibliography tools, `cryptocode`, TikZ,
  failures, risky configuration, Unicode, and portability.
- End-to-end tests exercise open, edit, save, build, preview, diagnostics, and navigation.

Golden tests assert stable semantic output rather than complete tool logs.
