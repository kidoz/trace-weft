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
test: test-rust

# Build the entire project
build: build-rust build-web

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
    cargo test --workspace --all-features

# Build Rust workspace
build-rust:
    cargo build --workspace --all-features

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

# Build Web App for production
build-web:
    cd apps/web && npm run build

# --- Desktop (Tauri) Commands ---

# Run the Desktop App in Development Mode
dev-desktop:
    cd apps/desktop/src-tauri && cargo tauri dev

# Build the Desktop App for production
build-desktop:
    cd apps/desktop/src-tauri && cargo tauri build
