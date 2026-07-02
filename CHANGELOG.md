# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `SpanBuilder::run_with` passes the closure a `SpanHandle` so response-derived
  data — token usage, cost, cache status, extra attributes — can be recorded on
  the span itself. Previously the builder's fields were frozen before the
  closure ran, so spans around real LLM calls could never carry
  `token_usage` / `cost_estimate` and the workbench's Input/Output/Cost tiles
  and token heatmap stayed empty. Handle values are merged on success and
  error, and take precedence over the builder's setters. The
  `openrouter-agent` example now records usage and cost this way.

### Fixed

- The web UI no longer draws a fake macOS title bar above the native one in
  the desktop app.
- The inspector's Input/Output/Cost tiles roll up token usage and cost across
  the selected span's subtree, so the root agent span shows the run's totals
  instead of dashes.
- The token heatmap no longer crashes the trace view when a span's token
  usage omits the empty `breakdown` map (which the SDK skips when
  serializing).
- The app shell now locks to the viewport (`h-screen`) instead of growing
  with content, so the trace graph's fit-to-view centers within the visible
  pane rather than an off-screen canvas that had to be scrolled to, and the
  inspector scrolls internally as designed.

## [0.3.5] - 2026-07-01

### Documentation

- The README now documents crates.io installation (`trace-weft = "0.3"`) instead
  of the old git-dependency instructions, and installs the unpublished CLI from
  the repository. Publishing this release makes the corrected README render on
  the crates.io pages.
- `DEVELOPMENT.md` points at the OpenAPI contract's relocated in-crate path
  (`crates/trace-weft-server/openapi/trace-weft.openapi.json`).

[0.3.5]: https://github.com/kidoz/trace-weft/compare/v0.3.4...v0.3.5

## [0.3.4] - 2026-07-01

### Security

- Updated `anyhow` to 1.0.103 to clear RUSTSEC-2026-0190, an unsoundness in
  `Error::downcast_mut()` (use-after-context borrow violation) present in
  1.0.102. The supply-chain CI gate denies the advisory, so the workspace now
  pins the patched release.

[0.3.4]: https://github.com/kidoz/trace-weft/compare/v0.3.3...v0.3.4

## [0.3.3] - 2026-07-01

### Fixed

- Published crates now carry a README on crates.io: every published crate
  declares `readme = "../../README.md"` so the project overview renders on its
  registry page.
- The `trace-weft-server` crate now packages cleanly: the OpenAPI contract it
  embeds was moved from the repo-root `schemas/api/` into the crate at
  `openapi/trace-weft.openapi.json`, so `include_str!` resolves inside the
  published tarball instead of reaching outside the package. The web API-type
  generator reads the contract from its new location.

[0.3.3]: https://github.com/kidoz/trace-weft/compare/v0.3.2...v0.3.3

## [0.3.2] - 2026-07-01

### Changed

- Trimmed unused dependencies flagged by `cargo-machete` across the desktop app
  and the CLI, core, ingest, MCP, OpenInference, OTel, and recorder crates.
- Hardened the supply-chain gate: `cargo-deny` now ignores advisories with no
  available fix and recognizes the workspace's private crates.

### Fixed

- The Rust CI job now installs the Tauri system dependencies so the desktop
  crate builds on a clean runner.

[0.3.2]: https://github.com/kidoz/trace-weft/compare/v0.3.1...v0.3.2

## [0.3.1] - 2026-06-26

### Features

- OTLP/HTTP JSON ingestion is now served at `POST /v1/traces` — payloads are decoded by `trace-weft-ingest` (original trace/span/parent IDs preserved, `400` for malformed bodies) and, like `/api/v1/batch`, the authenticated project is stamped onto every span before it is persisted. Resolves the 0.3.0 "OTLP is library-only" limitation.
- The web UI accepts an API key — a header field stores an `Authorization: Bearer` token (empty keeps the local dev bypass) that is sent on every request, so the workbench can talk to an authenticated, project-scoped server. Resolves the 0.3.0 "web UI has no API-key entry" limitation.

[0.3.1]: https://github.com/kidoz/trace-weft/compare/v0.3.0...v0.3.1

## [0.3.0] - 2026-06-26

### Added

- OpenAPI contract (`schemas/api/trace-weft.openapi.json`) and generated
  TypeScript API types consumed by the web client.
- Server query endpoints for replay-plan, trace-diff, replay-config, and the
  OpenAPI document.
- Web workbench: the Graphite design system, a structured span-metadata
  inspector, a waterfall time axis, a trace graph, transcript and content/blob
  views, trace-list search with root-span/model/error summaries, and a rework
  onto a typed API client backed by React Query.
- Query API serving full span detail, trace events, and blob content.
- Builder input/output capture by label under the active capture policy.
- Redaction of error messages (with real error type names), redacted previews
  for full-content policies, a fallback redactor, and secret-assignment, phone,
  and credit-card patterns.
- Desktop window capability and a content security policy.
- Project tooling: a CI workflow, workspace lints, `cargo-deny`/`cargo-machete`
  hygiene, Vitest unit tests, Playwright smoke tests, a docker compose Postgres,
  and a `just test-pg` recipe.
- A `DEVELOPMENT.md` development guide.

### Changed

- Raised the minimum supported Rust version to 1.96.
- Updated dependencies to the latest compatible versions.
- Restyled the web workbench to the Graphite design.
- Removed the unused Tauri shell plugin.

### Fixed

- Postgres backend: run the schema DDL unprepared and create it via the server's
  own pool, and store/decode `retry_count` as `BIGINT`. The backend previously
  failed on connect and was effectively non-functional.
- HITL approvals: upsert spans recorded twice with the same `span_id` so a
  resolved breakpoint persists its final state instead of staying stuck pending.
- Web: rebuild the trace-graph layout per trace to stop cross-trace drift, and
  surface fetch errors on the evaluations and trace-diff screens.
- Corrected the README CLI and OTLP descriptions to match what the server
  actually serves.

[0.3.0]: https://github.com/kidoz/trace-weft/compare/v0.2.0...v0.3.0
