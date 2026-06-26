# Default recipe
default:
    @just --list

# Format all code (Rust and TypeScript)
format: format-rust format-web

# Lint all code (Rust and TypeScript)
lint: lint-rust lint-web

# Check all code (Rust check and TypeScript build)
check: check-rust check-web

# Run all tests
test: test-rust test-web

# Build the entire project
build: build-rust build-web

# Supply-chain and dependency hygiene checks
hygiene: deny machete

# --- Rust Commands ---

# Format Rust workspace
format-rust:
    cargo fmt --all

# Lint Rust workspace using Clippy
lint-rust:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Check Rust workspace for compilation errors
check-rust:
    cargo check --workspace --all-targets --all-features

# Run Rust workspace tests
test-rust:
    @if cargo nextest --version >/dev/null 2>&1; then \
        cargo nextest run --workspace --all-features && cargo test --workspace --all-features --doc; \
    else \
        echo "cargo-nextest is not installed; falling back to cargo test."; \
        cargo test --workspace --all-features; \
    fi

# Run the Postgres-backed e2e tests against the docker compose Postgres.
# Start it first with: docker compose up -d postgres
test-pg:
    TRACE_WEFT_PG_TEST=1 \
    TRACE_WEFT_PG_URL=postgres://postgres:postgres@localhost:5432/trace_weft_test \
    cargo test -p trace-weft-server --test postgres_e2e

# Build Rust workspace
build-rust:
    cargo build --workspace --all-features

# Check Rust advisories, licenses, bans, and sources
deny:
    cargo deny check

# Check for unused Rust dependencies
machete:
    cargo machete

# --- Web (TypeScript/React) Commands ---

# Format Web App using Prettier
format-web:
    cd apps/web && npm run format

# Lint Web App using ESLint
lint-web:
    cd apps/web && npm run lint

# Check Web App (TypeScript compilation check)
check-web:
    cd apps/web && npm run build

# Run Web unit/component tests
test-web:
    cd apps/web && npm run test

# Run Web Playwright smoke tests
test-web-e2e:
    cd apps/web && npm run test:e2e

# Build Web App for production
build-web:
    cd apps/web && npm run build

# --- Desktop (Tauri) Commands ---

# Run the Desktop App in Development Mode
dev-desktop:
    cd apps/desktop/src-tauri && TRACE_WEFT_DEV_DIR=../../../.trace-weft npm --prefix ../../web exec -- tauri dev

# Build the Desktop App for production
build-desktop:
    cd apps/desktop/src-tauri && npm --prefix ../../web exec -- tauri build
