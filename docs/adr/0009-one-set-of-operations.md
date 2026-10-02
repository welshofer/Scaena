# ADR-0009: One set of operations, typed; the CLI and the MCP server only carry them

**Status:** proposed · **Date:** 2026-10-02

## Context

ADR-0003 says the CLI and an MCP server expose the same operations, with machine-readable results. SPEC §7.2 adds that each MCP tool's input and output schemas are generated from the Rust types.

Through PLAN 1.16, the operations lived in `scaena-cli`'s `main.rs`.
- Each command built its result with `serde_json::json!` and printed it, so no type described a result, and there was nothing to generate an output schema from.
- Errors were `anyhow`, which CLAUDE.md keeps to the binary.

An MCP server then had two ways to reach the operations, and both were poor:
- **Run the binary per tool call.** Each call is a process that reads the bundle and its fonts again. Results are typed only by convention, and failures come back as exit codes and stderr to parse.
- **Make `scaena-cli` a library.** That puts `clap` and `anyhow` into the server, and makes a client depend on another client.

## Decision

1. **`scaena-ops` holds the operations**, between the clients and the libraries: `cli`, `mcp` → `ops` → `export`, `paint`, `engine`, `store` → `core`.
   - Each operation is a function over a bundle:
     - make one (`create`), attach data to it (`attach`), and read it (`read`);
     - lint it, with and without fixes (`lint`, `lint_fix`);
     - patch, re-theme, inspect, diff, render, and export it;
     - read and replace its spine.
   - Each returns a typed result (serde, and schemars for its schema) or an `OpsError { message, plan?, op? }`, made with `thiserror`.
   - `render`'s wall-clock timings are taken here, outside the render path, which never reads a clock (SPEC §13).
2. **The CLI parses, calls, and prints.** Under `--json` it prints the result serialized, and otherwise as text. An `OpsError` exits 2, or 3 when it names the PLAN task that builds what it needs. Every CLI test passed unchanged across the move.
3. **The MCP server deserializes, calls, and returns.**
   - It reads a tool's arguments into a typed input.
   - It runs the operation on a blocking thread, since operations lay out and paint.
   - It returns the result as structured content.
   - Its schemas come from those types, through `rmcp`'s `#[tool]` macros and schemars. They are committed in `docs/schema/mcp/` and held there by a test, as ADR-0007 holds `docs/schema/*.json`.
4. **Operations an agent needs that no command had:**
   - `deck_create`: a bundle from a theme, the fonts its families name, data files, and a deck.
   - `data_attach`: a CSV or JSON file copied in and declared, its columns typed by inference.
   - `deck_read`, `spine_read`, `spine_update`.

   Without `deck_create`, an agent copies the theme's fonts and lists them in `fonts` by hand. That is authorability finding 5: the spike's deck listed two of its theme's three families, and could not render. `deck_create` takes the list from the theme. Without `data_attach`, an agent types each column by hand.

   These operations live in `scaena-ops`, so a command for any of them is a few lines when a person needs one.
5. **A tool that stops returns a tool result, not a protocol error.** It sets `isError`, and its text is `{ message, plan?, op? }`: the fields of the CLI's `--json` error object, minus the exit code. The agent sees what stopped it, and where.
6. **Resources are compiled in** with `include_str!`: the three schemas, SPEC, its lint catalog (§7.5, cut from it), the authoring skill, and the examples. The binary serves the documentation of the format it reads.
7. **Two inputs are typed loosely**: a patch's ops, and a deck to create. Each points at the resource that types it. Inlined, the patch schema would add 73 KB to every `tools/list`, which most clients put in front of the model on every turn. The server checks each op as `scaena patch` does and names the one that fails.

## Consequences

- **+** One implementation. A fix to an operation reaches the CLI and every tool, and the CLI tests and the MCP tests guard the same code.
- **+** Output schemas exist because results are types. A changed result shows up as a reviewed diff in `docs/schema/mcp/`.
- **+** The Mac client's FFI (PLAN 3.1) can wrap the same functions.
- **−** One more crate. It carries the engine, the CPU painter, and the exports, so `scaena mcp` is the CLI binary itself, not a smaller server.
- **−** Tool results differ from `--json` where a command prints a list or a map: MCP's structured content is an object, so `deck_lint` returns `{ findings, … }` where `lint --json` prints an array (SPEC §7.2).
- **−** A client cannot check a patch against its schema before sending it; the server does.
- **−** `scaena-ops` writes bundles to the filesystem. The browser assistant (PLAN 2.6) calls the same operations from WASM, where there is none. Reads already go through `scaena-store`'s `Files`; writes need the same seam by then.
