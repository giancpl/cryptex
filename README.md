# CrypTex

CrypTex is a Linux-first, local-first desktop environment for writing cryptography
papers in standard LaTeX. Project folders remain ordinary LaTeX projects that can
be compiled with `latexmk` or uploaded to Overleaf.

The project is in foundation development. See [PRODUCT](docs/PRODUCT.md),
[ARCHITECTURE](docs/ARCHITECTURE.md), [SECURITY](docs/SECURITY.md), and
[TASKS](docs/TASKS.md) before changing product behavior.

## Toolchain

- Node.js 22 LTS and pnpm 10
- Rust stable
- Tauri 2 system prerequisites

## Commands

```bash
pnpm install --frozen-lockfile
pnpm check
pnpm test
pnpm tauri build
```

`pnpm dev` starts the web UI. `pnpm tauri dev` starts the desktop application.

The D1 TeX Live spike can measure an official TLPDB and verify an isolated
installation without modifying the system:

```bash
node scripts/measure-texlive.mjs /path/to/texlive.tlpdb
scripts/verify-texlive-spike.sh /path/to/texlive/bin/x86_64-linux
```

See [ADR-002](docs/adr/002-managed-texlive-2026.md) for the pinned payload
decision, measured size, licensing gates, and remaining clean-machine checks.

## Status

M0 and local project editing through C6 are implemented. D1-D3 have frozen the
managed TeX Live direction, verified installed toolchains, added restricted process
execution, explicit build permissions, and deterministic root/engine resolution. E2
safe `latexmk` request construction is next. No production release exists.
