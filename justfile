# Scaena task runner. `just` with no args lists recipes.

default:
    @just --list

# fmt + clippy (-D warnings) + tests + schema validation. Must be green before any commit.
check: fmt-check clippy test schema

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

# Validate docs/examples against docs/schema (needs python3 + jsonschema; `pip install jsonschema`).
schema:
    python3 scripts/validate_schema.py .

# Run the CLI: `just cli lint docs/examples/revenue.deck.json`
cli *ARGS:
    cargo run -q -p scaena-cli -- {{ARGS}}

# Validate, lint, and inspect the example deck.
example:
    cargo run -q -p scaena-cli -- validate docs/examples/revenue.deck.json
    cargo run -q -p scaena-cli -- lint docs/examples/revenue.deck.json
    cargo run -q -p scaena-cli -- inspect docs/examples/revenue.deck.json

# Phase 0: parity harness (exists once PLAN 0.9 lands).
spike:
    @echo "PLAN 0.9: cargo test -p scaena-paint --features gpu -- parity" && cargo test -p scaena-paint -- parity

# Build the WASM engine (PLAN 0.8).
wasm:
    cargo build -p scaena-wasm --target wasm32-unknown-unknown --release

# Regenerate the Cargo.lock-resolved versions table for ADR-0004.
versions:
    cargo tree -p scaena-engine -p scaena-paint --depth 1 2>/dev/null || true
