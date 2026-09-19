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

## Status

M0 repository foundation is in progress. No production release exists.
