# Architecture

The React/TypeScript frontend owns presentation and transient view state. The
Tauri/Rust backend owns project identity, validated paths, filesystem operations,
watching, processes, compilation, indexing, and durable local settings.

The boundary consists of versioned serializable DTOs, typed commands, typed
events, stable error codes, and operation IDs. Frontend code never receives a
general filesystem or process primitive.

Project source folders contain only standard project files. CrypTex settings,
trust records, recovery snapshots, caches, and rebuildable indices live in the
platform application-data/cache locations keyed by canonical project identity.

See `DECISIONS.md` and `adr/` for binding architectural decisions.

## Managed toolchain boundary

D2 resolves TeX only from `app_data/toolchains/versions/<toolchain-id>` through
a strict versioned manifest and `active.json`. Every required executable is
canonicalized beneath that root, SHA-256 checked, and version-probed with a cleared
environment before the toolchain can be reported ready. Project paths and ambient
`PATH` are never discovery inputs. See ADR-002.

## Restricted process boundary

D3 keeps process creation inside the Rust core; there is intentionally no generic
frontend execution command. Callers select a named, pre-registered executable and
provide an argument array plus a working directory that is canonicalized beneath an
approved root immediately before spawning. The supervisor clears the inherited
environment, supplies only the managed binary directory and stable locale/timezone,
streams a bounded combined stdout/stderr budget, and enforces timeout, cancellation,
and resource limits. On Linux, each child starts in a fresh process group so
termination covers descendants.
