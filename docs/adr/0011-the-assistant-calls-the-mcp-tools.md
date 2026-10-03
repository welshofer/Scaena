# ADR-0011: The page's assistant calls the MCP tools on the bundle it holds

**Status:** proposed · **Date:** 2026-10-03

## Context

PLAN 2.6 asks for an assistant in the web editor: the user's own key (ADR-0006), Anthropic, OpenAI, and Gemini, and function calling onto the same operations as the CLI. Gate 2's fourth criterion is that it runs PLAN 1.19's agent loop in the browser.

Four facts shape it:
- **The operations are `scaena-ops`'** (ADR-0009), and the MCP server already gives them to agents as tools. Their schemas are generated into `docs/schema/mcp/`, and the skills are written against them.
- **The engine's module is near its budget.** SPEC §15 allows 3.0 MB gzipped, and the editor's module was 2.75 MB. Two things threaten that budget:
  - **Writing reaches the CRDT.** Each operation that writes records the change in the bundle's history, which is the CRDT. In the module that would be 0.95 MB more. A dry run does not help: what a module carries depends on what its code can reach, not on what a call does.
  - **What an agent reads is large.** The specification, the schemas, the skills, and the examples come to 0.13 MB gzipped.
- **A browser calls each provider directly.** Anthropic needs its opt-in header for that. OpenAI and Gemini answer a page's requests as they are.
- **Model names go stale.** A default compiled into the page would name a model that is gone within a year.

## Decision

1. **The tools are the MCP server's**, by the same names and arguments, on the bundle the page holds. Three arguments are left out: `bundle`, `out`, and `painter`. The page reads each tool's schema from `docs/schema/mcp/` at build time. `deck_create`, `theme_apply`, and `deck_export` are left out too: the page opens and downloads bundles itself. `resource_read` is added for what the server serves as resources, and for a skill the bundle carries.
2. **The operations that write each get a twin that writes nothing**: `patching`, `fixing`, `attaching`, and `spine_updating`. Each computes the deck to write, and why. The native operation calls its twin, then writes. The page calls only the twin, and writes into its session itself.
   - The page keeps no history yet (PLAN 2.9), so the CRDT stays out of its module.
   - A test of the module's tools (`scaena-wasm`'s `assistant`) holds each tool's arguments to its MCP schema.
3. **What an agent reads moves to a crate of its own**, `scaena-resources`. The MCP server serves it, as before. The page loads it as a WASM module of its own the first time the user asks the assistant something, with the assistant's code.
4. **The providers are called with `fetch`.** Each adapter maps one conversation into its provider's request and back, in about a hundred lines, with no SDK:
   - Anthropic's Messages API, with its header for direct browser calls;
   - OpenAI's Chat Completions, which other servers speak too, at an address the user gives;
   - Gemini's generateContent, its thought signatures sent back as they came.

   The page lists the models the key can use, and names none itself.
5. **Keys stay in the page.** A key is kept for the tab by default. When the user asks, it is kept on the device, encrypted with a key the browser keeps and never hands out. It goes to its provider and nowhere else.

## Consequences

- **+** An agent over MCP and the assistant in the page have the same tools, the same results, and the same skills. A tool added to the server reaches the page through its generated schema and one arm of `Session::tool`.
- **+** The editor's module carries the assistant's tools and still shrank, from 2.75 MB to 2.73 MB gzipped. The tools added 0.6 MB of code before gzip, and two changes took out more:
  - Each crate that parsed a theme had compiled its own copy of the theme model's parser. Every crate now calls one function in `scaena-core`.
  - Two paths parsed the deck from a JSON value. They now go through its text.
- **+** The page carries nothing a provider ships, and works with any model the key can use, including models that come out later.
- **−** The page's edits are recorded nowhere until a client that records writes the deck (PLAN 2.9), as for every edit the page makes.
- **−** A tool lints and lays out as the CLI does, from the bundle's files, and a patch lints the deck before and after. On B1 (40 states), in headless Chromium, a patch takes 2.5 s, a lint 1.2 s, and a render 1.3 s after an edit. That is slow beside typing, and quick beside a model's answer. Reusing the editor's last lint as the patch's "before" would halve it.
- **−** A real key is the only full test: CI runs the loop against scripted servers for each wire format (`web/assistant.mjs`).

## Alternatives

- **The providers' SDKs.** Rejected:
  - Each would add its own weight.
  - Each would assume a server, or need a flag to run in a browser.
  - A fourth provider would need a fourth SDK.
- **A proxy that holds the keys.** It would need a backend, which ADR-0006 rules out. Rejected.
- **Tools of the page's own**, such as one that edits `.scn` text. They would drift from the operations, and the skills would not describe them. Rejected: the model edits the deck with the ops an MCP agent uses, and the editor shows each edit as source.
- **The resources in the engine's module.** Every page that never asks the assistant anything would carry 0.13 MB more, close to the budget. Rejected.
- **A dry run, then the page applies the patch itself.** The write path would still be compiled in, so the CRDT with it. The page would also repeat what the operation decides. Rejected.
