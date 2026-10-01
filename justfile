# Scaena task runner. `just` with no args lists recipes.

default:
    @just --list

# fmt + clippy (-D warnings, all features) + tests + schema + wasm32. Must be green before any commit; mirrors CI.
check: fmt-check clippy test schema wasm-check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test:
    cargo test --workspace --locked

# The engine and both painters must keep compiling for the browser (PLAN 0.1, 0.8).
wasm-check:
    cargo clippy -p scaena-engine -p scaena-paint --all-features --target wasm32-unknown-unknown --locked -- -D warnings

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

# Print the Cargo.lock-resolved versions behind ADR-0004's table, then any duplicated crates.
versions:
    cargo tree --workspace --all-features -e normal --prefix none | grep -E '^(parley|parley_data|harfrust|skrifa|read-fonts|fontique|icu_segmenter|icu_properties|taffy|kurbo|peniko|linebender_resource_handle|vello|vello_shaders|wgpu|naga|vello_cpu|vello_common|glifo|fearless_simd) v' | sed 's/ (\*)//' | sort -u
    cargo tree --workspace --all-features -e normal -d --depth 0
