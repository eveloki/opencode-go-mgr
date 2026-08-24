[简体中文](development.zh-CN.md)

# Development

## Prerequisites

Use Node.js 22 (the CI baseline), pnpm 10.29.2 (`packageManager` in
`package.json`), and Rust 1.85 or newer. Native build dependencies vary by
runner; treat `.github/workflows/release.yml` as the source of truth. The
current Linux runner installs `libwebkit2gtk-4.1-dev
libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf
libfuse2 xvfb xauth xdg-utils dbus-x11`.

## Development

Exit any running release tray app so the single-instance lock and port `9042`
are free, then start the full development stack:

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` runs `tauri dev`. On Windows the `predev` script
(`scripts/free-dev-port.mjs`) inspects `127.0.0.1:30001` and stops any stale
Vite process from a previous run. Tauri starts Vite and waits for the gateway
to be ready, then opens `http://127.0.0.1:30001/dashboard/`. Vite proxies
`/dashboard/api` (including WebSockets) to `http://127.0.0.1:9042`.

- Frontend (Vue, CSS, TypeScript) changes use Vite HMR.
- Rust changes use Tauri's watcher plus Cargo's incremental compiler, then
  restart the process. Rust code is **not** replaced inside a running
  process — expect a restart.

After cloning, enable the shared git hooks once (also runs from `pnpm install`
via the `prepare` script):

```bash
pnpm run hooks:install
# equivalent: git config core.hooksPath .githooks
```

When a commit stages any `*.rs` file, `.githooks/pre-commit` runs
`cargo fmt --all` and re-stages those Rust files so the commit stays
rustfmt-clean (same tool CI checks with `cargo fmt --all -- --check`).

## Checks And Builds

```bash
pnpm install
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run contract:v3:check
pnpm run build
```

- `pnpm run build:web` is the **frontend-only** production build
  (`vue-tsc && vite build`). Use it when you only need to validate the
  dashboard.
- `pnpm run test` runs `pnpm run test:web` (Node `--experimental-strip-types`
  over `scripts/*.test.mjs` and `src/**/*.test.ts`), `vue-tsc --noEmit`,
  `vite build`, then `cargo test --workspace --locked`.
- `pnpm run test:rust` is the locked workspace Rust suite by itself.
- `pnpm run contract:v3:check` regenerates the Dashboard V3 JSON Schema from
  `ocg-core`'s `export_dashboard_v3_schema` example and fails if
  `schema/dashboard-api-v3.schema.json` or
  `src/api/generated/dashboard-v3.ts` drifted. Write with
  `pnpm run contract:v3:generate`.
- `pnpm run design:lint` runs the `@google/design.md` linter against
  `DESIGN.md`.
- `pnpm run build` is reserved for **release validation**. It runs
  `scripts/release.mjs`, which builds the current supported native platform
  and atomically replaces `release/` only after every expected file passes
  validation. The previous `release/` is preserved on failure. Cargo's
  incremental build cache is **not** erased. Release binaries use thin LTO
  (`[profile.release]` in the workspace `Cargo.toml`) so native CI linking
  stays bounded.

## Rust Checks

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --locked
```

The first command checks formatting without changing files. Run
`cargo fmt --all` to apply formatting. With hooks enabled, staged Rust
commits auto-run that format step via `.githooks/pre-commit`.

For focused work:

```bash
cargo test -p ocg-domain
cargo test -p ocg-gateway
cargo test -p ocg-infra
cargo test -p ocg-core
cargo test -p ocg-manager-cli
cargo test -p ocg-browser-worker
cargo test -p ocg-manager --lib
cargo test -p ocg-core gemini
cargo test -p ocg-core claude_desktop
cargo test -p ocg-core dashboard_v3
cargo test -p ocg-core v3_runtime_invariants
```

`ocg-domain` / `ocg-gateway` crates compile their production-source
dependency and purity guards as ordinary `cargo test` cases. Host
characterization lives in `crates/ocg-core/tests/fixtures/v3/requirement_map.md`
and the copy at `src-tauri/tests/fixtures/v3/host_requirement_map.md` /
`crates/ocg-cli/tests/fixtures/v3/host_requirement_map.md`.

Run the CLI in a sandbox first when testing real account flows:

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

The CLI surface is `serve` / `key` / `status` only. `key add` creates an
enabled ready OpenCode Go card through `account_control::create_go_api_key`
and bumps that process's `settings_revision`. It cannot create Custom
accounts, sub keys, or settings. Direct `Database::update_account` still
does not bump revision; that is intentional and is not the CLI path.

## Frontend Checks

Frontend unit tests live next to the code they cover (`src/**/*.test.ts`)
and run with Node's experimental `--experimental-strip-types` flag — no
extra test runner is required. Script-level tests live in
`scripts/*.test.mjs` (release helpers, Dashboard V3 contract, container
publish). Pair them with `pnpm run build:web` and
`pnpm run contract:v3:check`.

The application guides are driven by the 16 entries in
`src/views/application-guides.ts`. When changing that registry, check the
guide count, unique IDs, protocol endpoints, the display/copy masking
difference, and the Claude Desktop three-role persistence behavior.

The side rail is Dashboard / Access Keys / Accounts / Providers /
Applications / Logs / Settings. A `pricing` query is a legacy alias for
Providers. `BrowserSession` is a session overlay, not an eighth rail item.
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](development.zh-CN.md) · [Docs index](../README.md)
