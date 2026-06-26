# Development Workflow

TraceWeft uses an explicit Rust workspace policy:

- Rust edition: 2024
- MSRV: Rust 1.96, from `[workspace.package].rust-version`
- Cargo resolver: `resolver = "2"`
- Default workspace members: every crate, example, and the Tauri shell
- Workspace lints: configured in root `Cargo.toml` and inherited by each Rust package

## Local Checks

Core checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm --prefix apps/web run lint
npm --prefix apps/web run test
npm --prefix apps/web run build
```

Preferred fast test runner:

```bash
cargo nextest run --workspace --all-features
```

Install optional Rust workflow tools:

```bash
cargo install cargo-nextest cargo-deny cargo-machete
```

Supply-chain and dependency hygiene:

```bash
cargo deny check
cargo machete
```

CI uses `sccache`; local developers can opt in with:

```bash
export RUSTC_WRAPPER=sccache
```

## API Contract

The local API contract lives at `schemas/api/trace-weft.openapi.json` and is also
served by the server at `/api/openapi.json`.

Generate the React TypeScript API types after editing the contract:

```bash
npm --prefix apps/web run generate:api
```

The generated file is `apps/web/src/generated/api-types.ts`; do not edit it by
hand.

## UI Tests

Component tests use Vitest and Testing Library:

```bash
npm --prefix apps/web run test
```

Browser smoke tests use Playwright:

```bash
npm --prefix apps/web exec playwright install chromium
npm --prefix apps/web run test:e2e
```
