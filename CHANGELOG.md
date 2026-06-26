# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
