# Development

Use Node 22 LTS, pnpm 10, stable Rust, and the Tauri 2 Linux prerequisites. Exact
package manager inputs are locked. Do not use globally installed frontend tools in
CI.

Run `pnpm check` before `pnpm test`; run `pnpm tauri build` for desktop-affecting
changes. Generated boundary files are checked for drift. Toolchain discovery and
packaging decisions remain gated by task D1.

`pnpm check` and `pnpm rust:test` validate the platform-independent Rust core.
`pnpm desktop:lint` and `pnpm desktop:test` additionally require the WebKitGTK 4.1
development packages listed by Tauri. CI runs both layers.
