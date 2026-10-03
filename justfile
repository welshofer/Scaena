# Scaena task runner. `just` with no args lists recipes.

default:
    @just --list

# fmt + clippy (-D warnings, all features) + tests (CPU and GPU) + schema + wasm32. Must be green before any commit; mirrors CI.
check: fmt-check clippy test test-gpu schema scripts wasm-check

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

# Re-bless the generated schemas (PLAN 1.1) and the MCP tools' (PLAN 1.17), then the torture goldens:
# display lists first, then the rasters painted from them. Only after reviewing the diffs in docs/schema/
# and tests/golden/**/actual/ (written by a failing `just test`).
bless:
    SCAENA_BLESS=1 cargo test -p scaena-core --test schemas --locked
    SCAENA_BLESS=1 cargo test -p scaena-mcp --test schemas --locked
    SCAENA_BLESS=1 cargo test -p scaena-engine --test torture --locked
    SCAENA_BLESS=1 cargo test -p scaena-paint --test torture_rasters --locked
    rm -rf tests/golden/torture/actual

# The engine, both painters, and the WASM bindings must keep compiling for the browser (PLAN 0.1, 0.8).
wasm-check:
    cargo clippy -p scaena-engine -p scaena-paint -p scaena-wasm -p scaena-subset --all-features --target wasm32-unknown-unknown --locked -- -D warnings
    cargo clippy -p scaena-wasm --no-default-features --features gpu,cpu --target wasm32-unknown-unknown --locked -- -D warnings

# Validate examples and fixture bundles against docs/schema, and check torture-deck font coverage
# (needs python3; `pip install jsonschema fonttools==4.66.1`).
schema:
    python3 scripts/validate_schema.py .
    python3 scripts/build_torture_fonts.py --check
    python3 scripts/build_torture_images.py --check
    python3 scripts/build_bundle_fonts.py --check
    python3 scripts/build_bench_decks.py --check

# The scripts' own tests: the bench gate's judgment (PLAN 1.24).
scripts:
    python3 -m unittest discover -s scripts -p 'test_*.py'

# CI runs these on each runner, and fails a pull request on a bench slower than its base, timed
# beside it on the same machine, by more than the run's floor every time: 10%, or 2.5 times the
# run's noise if that is more (.github/workflows/bench.yml, scripts/bench_gate.py).
# SPEC §15's stages on B1–B4, timed by criterion (PLAN 1.24); `just bench layout/b1` runs one.
bench *FILTER:
    cargo bench --locked -p scaena-cli --features gpu --bench stages -- {{FILTER}}

# Per-state medians and worst cases on one bundle, in one process (PLAN 0.14's tables).
stages BUNDLE="tests/bench/b1.scaena":
    cargo build --release --locked -p scaena-cli --features gpu --bins --examples
    target/release/examples/stages {{BUNDLE}}

# B1's cold start in headless Chromium: WASM load to the first frame at 1080p (SPEC §15).
coldstart: wasm
    node crates/scaena-wasm/www/coldstart.mjs tests/bench/b1.scaena

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

# The parity harness (PLAN 0.9): every torture state from vello_cpu (the goldens), vello on this
# machine's GPU, and vello on WebGPU in headless Chromium, compared pairwise; diff images for
# failing pairs land in tests/golden/torture/actual/.
spike: wasm-smoke
    SCAENA_WEB_PNGS={{justfile_directory()}}/target/wasm-smoke cargo test -p scaena-paint --features gpu --test parity --locked -- --nocapture

# Build the WASM engine and its JS glue into crates/scaena-wasm/www/pkg (PLAN 0.8), the font
# subsetter, which a page loads to download a bundle, into crates/scaena-subset/pkg (PLAN 2.4),
# and the player's engine alone, without the editor's operations, into crates/scaena-wasm/player:
# what a single-file HTML export carries (PLAN 2.5). Cargo keeps each feature set's build, so
# building one after the other rebuilds neither.
# Needs `cargo install wasm-bindgen-cli --version 0.2.129` (the version in Cargo.lock).
wasm:
    cargo build -p scaena-wasm -p scaena-subset --target wasm32-unknown-unknown --release --locked
    wasm-bindgen --target web --out-dir crates/scaena-wasm/www/pkg target/wasm32-unknown-unknown/release/scaena_wasm.wasm
    wasm-bindgen --target web --out-dir crates/scaena-subset/pkg target/wasm32-unknown-unknown/release/scaena_subset.wasm
    cargo build -p scaena-wasm --no-default-features --features gpu,cpu --target wasm32-unknown-unknown --release --locked
    wasm-bindgen --target web --out-dir crates/scaena-wasm/player target/wasm32-unknown-unknown/release/scaena_wasm.wasm

# The WebGPU page in headless Chromium: WASM display lists hash to the native digests and
# every torture state paints (PLAN 0.8). Needs Node and Playwright with its Chromium.
wasm-smoke: wasm
    node crates/scaena-wasm/www/smoke.mjs

# The web player (PLAN 2.1): the WASM engine, then the Vite app into web/dist, and the page a
# single-file export fills in (PLAN 2.5) into crates/scaena-export/player, which `scaena` built
# after it carries. Needs Node 22. Serve the repository's root and open
# /web/dist/?bundle=/tests/fixtures/torture.scaena.
web: wasm
    cd web && npm ci && npm run build

# The web player on Vite's dev server, with the repository's bundles at their paths in it
# (/?bundle=/docs/examples/ridgeline.deck.json), on the WASM engine `just wasm` last built.
web-dev:
    cd web && npm run dev

# The web player in headless Chromium: every torture frame shown, painted in its worker by
# WebGPU and by the CPU painter (PLAN 2.1); a single-file export's, from its address on disk
# with the network off (PLAN 2.5); and the parity harness holds all three to the goldens. Then
# the player's controls and presenter view (PLAN 2.2), the source editor (PLAN 2.3), and its
# storage: open, save, download, and drop (PLAN 2.4).
web-smoke: web
    node web/smoke.mjs
    node web/standalone.mjs
    SCAENA_WEB_PNGS={{justfile_directory()}}/target/web-smoke/player-webgpu:{{justfile_directory()}}/target/web-smoke/player-cpu:{{justfile_directory()}}/target/web-smoke/standalone-cpu cargo test -p scaena-paint --test parity --locked -- --nocapture
    node web/player.mjs
    node web/editor.mjs
    node web/storage.mjs

# Print the Cargo.lock-resolved versions behind ADR-0004's table, then any duplicated crates.
versions:
    cargo tree --workspace --all-features -e normal --prefix none | grep -E '^(parley|parley_data|harfrust|skrifa|read-fonts|fontique|icu_segmenter|icu_properties|taffy|hypher|kurbo|peniko|linebender_resource_handle|vello|vello_shaders|wgpu|naga|vello_cpu|vello_common|glifo|fearless_simd) v' | sed 's/ (\*)//' | sort -u
    cargo tree --workspace --all-features -e normal -d --depth 0
