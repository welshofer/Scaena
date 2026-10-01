# Scaena task runner. `just` with no args lists recipes.

default:
    @just --list

# fmt + clippy (-D warnings, all features) + tests (CPU and GPU) + schema + wasm32. Must be green before any commit; mirrors CI.
check: fmt-check clippy test test-gpu schema wasm-check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test:
    cargo test --workspace --locked

# Painter and CLI tests with vello on the GPU (PLAN 0.7). Without an adapter they skip and say
# so; SCAENA_REQUIRE_GPU=1 makes that a failure (CI sets it; Linux CI uses Mesa's lavapipe).
test-gpu:
    cargo test -p scaena-paint -p scaena-cli --features gpu --locked

# Re-bless the torture goldens: display lists first, then the rasters painted from them. Only
# after reviewing the diffs in tests/golden/**/actual/ (written by a failing `just test`).
bless:
    SCAENA_BLESS=1 cargo test -p scaena-engine --test torture --locked
    SCAENA_BLESS=1 cargo test -p scaena-paint --test torture_rasters --locked
    rm -rf tests/golden/torture/actual

# The engine and both painters must keep compiling for the browser (PLAN 0.1, 0.8).
wasm-check:
    cargo clippy -p scaena-engine -p scaena-paint --all-features --target wasm32-unknown-unknown --locked -- -D warnings

# Validate examples and fixture bundles against docs/schema, and check torture-deck font coverage
# (needs python3; `pip install jsonschema fonttools==4.66.1`).
schema:
    python3 scripts/validate_schema.py .
    python3 scripts/build_torture_fonts.py --check

# Rebuild the torture deck's subset fonts from pinned upstream files (network; PLAN 0.2).
torture-fonts *ARGS:
    python3 scripts/build_torture_fonts.py {{ARGS}}

# Run the CLI: `just cli render tests/fixtures/torture.scaena --state pretty --out pretty.png`
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
