# Lift-Up Plan: Scaena

> Baseline: `main`@`e987ee170cb363bc41b79e27d9c31fa88cad872f` (2026-11-13), branch `welshofer-lift-up-audit-20261124`; clean tree; `origin/main` not fetched.
> Platform: mixed, a Rust core (`crates/`), the browser player and editor (`web/`), the Mac and iPad apps (`apps/`). Scope: full repo, weighted to code changed since the 20261110 plan (430 commits). Excludes everything `lift-up-plan-20261031.md` and `lift-up-plan-20261110.md` list, their Deferred lists included.
> Deadline: 20 minutes from 09:08:51 UTC. The survey stopped at about 10 minutes, when the user asked for `/burn-down`; coverage is partial (below).
> Owner: Claude Opus 5.5 (`claude-opus-5.5`), the session's own model, kept as the user's choice. The skill prefers GPT-6 Astra; this run did not switch. No survey agents: the host gives no way to cancel one, so the survey was inline.
> Model catalog checked 2026-11-24 at developers.openai.com's latest-model guide: GPT-6.2 Astra and GPT-6.2 Sol are newest, past the skill's 2026-09-30 catalog. The host's task tool lists `gpt-6.2-astra`, `gpt-6.2-sol`, `gpt-6.1-sol`, and `gpt-6-luna`, which is advertised support, not proven access.
> Attractiveness anchor: Keynote, inferred as in the earlier plans; no item this run.
> Selected: 4; ready: 4; needs-design: 0; deferred: 0. Contract and evidence: `~/.copilot/session-state/66c9c28e-da9c-4f3b-a2c7-f3a81d1c98ad/files/lift-up-plan-20261124.json` and `evidence/`, validated with `plan_contract.py --check-sources`.

## Performance

### PERF-1. Every PDF source is read twice as a deck starts being made, in the browser, on the Mac, and in the CLI
**Kind / readiness:** defect / ready
**Location:** `crates/scaena-generator/src/lib.rs:297-305` (`Generation::new`); `crates/scaena-ffi/src/generate.rs:386-390`; `crates/scaena-ops/src/narrative.rs:706` (`grounding`)
**Evidence:** each caller calls `scaena_sources::pdf::each(..)` and `scaena_sources::pdf::held(..)` on the same sources, and each of those runs `words()` on every PDF (`pdf.rs:77`, `:92`). Source SHA `0077562059bf…`.
**Do / why:** one function reads each PDF once, giving each PDF's words by name and why each unread one was not; the held words are joined from it. A long report (the reader allows a few hundred pages) is inflated and parsed once, not twice, before the first request. No timing is claimed.
**Acceptance:** given a readable PDF, an unreadable one, and a source that is no PDF, the new function gives the same words, joined text, and reason that `each` and `held` gave (unit test). `grep -nE 'pdf::(held|each)\('` over the three callers exits 1. Negative control: on the baseline it finds six calls.
**Preserve / non-goals:** same held text and figures; `unread` still said; the editor's module carries no PDF reader. Not caching across generations, nor `grounded_words`.
**Write scope / dependencies / priority:** `pdf.rs`, `scaena-generator/src/lib.rs`, `scaena-ffi/src/generate.rs`, `scaena-ops/src/narrative.rs`; after SEC-1 (same file); 3.
**Suggested implementation tier / reason:** fast; mechanical, with an equivalence oracle.
**Effort / impact:** S / S

## Functionality

### FUNC-1. The Mac and the iPad cannot walk through code or focus its lines from the caret: the C ABI never passes `focusing` or `walking`
**Kind / readiness:** enhancement / ready
**Location:** `crates/scaena-ffi/src/lib.rs:2033-2035` (`call`'s dispatch); `apps/mac/ScaenaKit/Sources/ScaenaKit/Clipboard.swift`
**Evidence:** the dispatch passes `coding`, `uncoding`, and `tidying`; a search for `focusing` or `walking` finds them only in `scaena-wasm` (`lib.rs:807`, `:814`). An unknown name answers "`walking` is not a call this library answers". Source SHA `61c76ec4242d…`.
**Do / why:** pass both through the C ABI, wrap them in ScaenaKit, and offer Focus on These Lines, Clear Focus, and Walk Through Lines… where a code text is selected, each one step to undo. PLAN 2.140, still open, names them for the editors and names ScaenaKit's `aWalkThroughAddsAStepForEachRange`. Today only the browser, the CLI, and MCP walk through code.
**Acceptance:** given a ScaenaKit session on a slide with code, `walking` with two ranges adds a step for each range after the first, from whole block to whole block, in one patch, and undo gives the deck back. Negative control: the call answers an error today.
**Preserve / non-goals:** Make Code, Not Code, Tidy Code, and the inspector's Lines in Focus as they are. Not Magic Move (2.141), and no engine change.
**Write scope / dependencies / priority:** `scaena-ffi/src/lib.rs`, `scaena-ffi/tests/ffi.rs`, `Clipboard.swift`, `NodeCommands.swift`, `CanvasMenus.swift`, `DeckView.swift`, new `CodeWalkTests.swift`; none; 4.
**Suggested implementation tier / reason:** strong; Rust FFI plus SwiftUI menus and a sheet, tested natively.
**Effort / impact:** M / M

## Stability

No item. Byte slices in `health.rs` and `narrative.rs` were checked (Dropped).

## Reliability

### REL-1. `deck_generate` fills a deck_create bundle without the bundle's write lock, so a call made at once on it is written over
**Kind / readiness:** defect / ready
**Location:** `crates/scaena-mcp/src/lib.rs:2061-2117` (`Scaena::deck_generate`)
**Evidence:** `fillable(&bundle)` reads the bundle on the async task, and `from_slides(.., blank.as_ref())` then the history's `begin` write it inside `blocking`, with no `open_by` or `writing`. Every other tool that writes takes `open_by` (lines 1479–2018). Source SHA `5c01ba77fbe5…`.
**Do / why:** take `writing(&bundle)` before `fillable` and hold it through the write. Since #1323 deck_generate writes over an existing bundle. An agent's parallel `data_attach`, `deck_patch`, `slide_write`, or second `deck_generate` on it can land between the read and the write and be lost while it reports success, as the 20261024 plan's REL-2 found for patches.
**Acceptance:** given a blank bundle deck_create made, when deck_generate fills it while several `data_attach` calls are made at once, then every call succeeds and the final deck declares every attached source. A new multi-thread test beside `writes_to_one_bundle_at_once_each_land`. Negative control: run before the fix, it loses the attaches that land in the fill's window.
**Preserve / non-goals:** a new bundle is made as before; tools that only read take no lock; refusals unchanged. Not deck_create's vacancy check, nor locking between processes.
**Write scope / dependencies / priority:** `crates/scaena-mcp/src/lib.rs`, `crates/scaena-mcp/tests/mcp.rs`; none; 1.
**Suggested implementation tier / reason:** strong; a small concurrency fix where a wrong guard scope still races or deadlocks.
**Effort / impact:** S / M

## Security

### SEC-1. ASCII85 and ASCIIHex output escapes the PDF reader's inflation bounds, and ASCII85's `z` makes four bytes of one
**Kind / readiness:** defect / ready
**Location:** `crates/scaena-sources/src/pdf.rs:336-382` (`File::data`), `ascii85` at `:1356`
**Evidence:** only Flate's output is checked against `bounds.inflated` and added to `self.total`. `ascii85(&data)` and `ascii_hex(&data)` are neither, and `b'z' if n == 0 => out.extend_from_slice(&[0; 4])`. Source SHA `fcc7adde52d9…`.
**Do / why:** hold every filter's output to a stream's bound and count it toward the total. Stop ASCII85 decoding past the bound, so it is refused before it is allocated. `/Filter [/FlateDecode /ASCII85Decode]` turns Flate's 64 MB into 256 MB that nothing counts, and forms nest 8 deep while each parent's content is held. By reading the bounds, a file of a few hundred KB can make the reader hold on the order of 1 GB; this was not measured. A PDF reaches the reader from the CLI, MCP, the C ABI, and the browser's generator.
**Acceptance:** within the tests' `SMALL` bounds, a page of `/A85` with 300 KB of `z` is refused, "a stream inflates past 1 MB". Three such streams of 200 KB are refused, "its streams run past 2 MB together". Dogfood 17's report (ASCII85 over Flate) and the other tests read as before. Negative control: both hostile files read today.
**Preserve / non-goals:** valid PDFs read the same; Flate's message unchanged. No new filters, and no change to `BOUNDS`.
**Write scope / dependencies / priority:** `crates/scaena-sources/src/pdf.rs`; none; 2.
**Suggested implementation tier / reason:** strong; a security bound on untrusted input that must keep real reports reading.
**Effort / impact:** S / M

## Usability

No item: not surveyed this run (coverage).

## Attractiveness / Sexiness

No item: not surveyed this run (coverage).

## Coverage

Read: the MCP server's tools and locks; the PDF reader's bounds, filters, CMaps, and forms; the PDF callers in the three clients; the Mac's re-theme and theme edit made away from the window; code-walk parity across the C ABI, ScaenaKit, and the browser; a spot check of `innerHTML` sinks in `story.ts`, `files.ts`, and `reading.ts`, and of byte slicing in `health.rs` and `narrative.rs`. Not surveyed: the web editor beyond that spot check, the iPad, health's heuristics, the generator's prompts, and the Stability, Usability, and Attractiveness lenses at large.

## Dropped during verification

- **Byte slices that panic on non-ASCII text** (`health.rs:1923`, `narrative.rs:8703`): both slice strings built from ASCII English words ("since", "after", "from"; health's own sentences), so neither can split a character.
- **A Mac re-theme dropped in silence when the deck keeps changing** (`DeckEditor.rethemeAway`): `DeckView.swift:2244-2247` says "not re-themed: the deck kept changing while it was made".
- **XSS through the Story's and Files' `innerHTML`**: every interpolated deck string checked goes through `html()`, and the reader's HTML comes from the engine. A spot check, not exhaustive.
- **A CMap range that maps without bound** (`pdf.rs:1300-1306`): each range is held to 0xFFFF codes and the whole to `most`.

## Needs design / Deferred

None selected.

## First move

**REL-1.** It is the one finding where an agent's work is lost while the server says it was made: the regression arrived with #1323, the fix is a lock every other writer already takes, and the at-once test it extends already exists.

## Burn-down manifest, 2026-11-24 (run 220558ee, stopped)

> Grant: `/burn-down` with no other text, right after `/lift-up`. Read as approval to implement this plan's ready items locally and to open a PR; no merge or deploy. Default limits: 60 minutes in all, 30 per item, three items per batch, two reworks per item, 800 changed lines per item, and a checkpoint every 10 minutes.
> Mode: bounded and owner-driven. This host has no way to cancel an agent, so there were no autonomous workers.
> Executor: Claude Opus 5.5 (`claude-opus-5.5`). Reviewers: read-only `code-review` agents asked for `gpt-6.2-astra`, which the host's status reports; the agents could not name themselves.
> Checkpoints: 09:21 (start), 09:29, 09:39, 09:47, 09:54 UTC (stopped, ~33 minutes used).

| Item | State | Reworks | What happened | What finishes it |
|---|---|---|---|---|
| REL-1 | blocked | 2 of 2 | Before the fix, the new at-once test failed: `deck_generate` errored writing `history/deck.loro` while attaches wrote. The first fix passed acceptance five times, the MCP suite, clippy, and fmt. Review rejected it: holding the lock across the wait for a blocking thread can deadlock Tokio's blocking pool, and a two-thread pool did hang on it for over five minutes. The rework takes the lock inside one blocking closure, as `open_by` does. Its new two-thread test then failed on its own bug: the timeout was made outside the runtime ("no reactor running"). | WIP is at `refs/wip/lift-up-20261124-REL-1` (`e8d82cb`). Build the timeout inside the runtime (`block_on(async { timeout(…).await })`), run it again, and get it reviewed; the reworked lock has not been reviewed. |
| SEC-1 | blocked | 2 of 2 | Before the fix, a page of `/A85` with 300 KB of `z` read as valid. Review 1 asked for three changes: decode ASCIIHex in one bounded pass, cap ASCII85's capacity, and skip images before decoding them. Review 2 found two regressions in that rework: `ascii_hex` reserves half its input even for string tokens, and the Form check deep-clones the XObject on every `Do`. | WIP is at `refs/wip/lift-up-20261124-SEC-1` (`f163c27`). Start `ascii_hex` empty, read `/Subtype` from the cached object by reference before `data`, then get it reviewed. |
| PERF-1 | planned | 0 | Not claimed: it depends on SEC-1. | SEC-1 integrated. |
| FUNC-1 | planned | 0 | Not started. It is M, with native Swift work, and did not fit the time left. | A run of its own. |

Shipped: nothing. No PR, no merge, no deployment. This plan and its manifest are local commits on `welshofer-lift-up-audit-20261124`. The ledger, receipts, reviews, and WIP patches are in the session folder (`run.json`, `checkpoint-*.json`, `REL-1-*`, `SEC-1-*`).

## Burn-down manifest, run 2 (3b40433f), 2026-11-24 20:50 UTC

> Grant: the user's choice in the app's form after run 1 stopped, "New run: finish REL-1 and SEC-1, then PERF-1 (45 min, 2 reworks each), and open a PR". A new contract (r2) carries run 1's review findings into REL-1's and SEC-1's acceptance; run 1's ledger keeps its own counters. No merge or deploy.
> Mode: bounded and owner-driven. Executor: Claude Opus 5.5 (`claude-opus-5.5`). Reviewers: read-only `code-review` agents asked for `gpt-6.2-astra`, which the host's status reports.
> `origin/main` was still `e987ee1` when the run began, eleven hours after run 1 stopped.

| Item | State | Reworks in run 2 | Commit | Acceptance evidence | Review |
|---|---|---|---|---|---|
| REL-1 | pr-open | 0 | `ef3dc32` | Before the fix, the at-once test failed: `deck_generate` errored writing `history/deck.loro`. Against run 1's lock placement, the two-thread test fails at 60 s; it now passes. Acceptance ran three times, plus the MCP suite, clippy, and fmt. | approved |
| SEC-1 | pr-open | 0 | `8a00d65` | Before the fix, an `/A85` page of `z` read as valid. Now ASCII85 alone and over Flate, three ASCII85 streams together, and an ASCIIHex stream are refused at their bounds. A page showing a 2 MiB image twice reads. The decoders stop at their bound and hold no room for white space. The ReportLab report reads as before. The crate's suite, clippy, and fmt pass. | approved on its third review |
| PERF-1 | pr-open | 1 | `a7561c3` | On the baseline the callers' grep finds six calls; it finds none now, and one `pdf::read` in each caller. Two real PDFs, a source that is no PDF, and an encrypted PDF, given out of name order, read once with the same words, joined text, and reason. The sources, generator, and FFI `generate` tests pass, with clippy (native and wasm32) and fmt. | approved on its second review; the first asked that SPEC §7.1.6's `pdf::each` stay true, so it is kept, as `read(..).0` |
| FUNC-1 | not in this grant | – | – | – | – |

Combined tree (wrapped receipt at 21:17 UTC): 118 tests pass and none fail across `scaena-mcp`, `scaena-sources` (pdf), `scaena-generator`, and the FFI's `generate`, with clippy `-D warnings` on seven crates and fmt. `just check` was not run; CI runs it.

Shipped: PR #1360, not merged; merging waits on your review and CI. Deployment: none. FUNC-1 waits for a run of its own. The ledgers, receipts, and reviews are in the session folder (`run-2.json`, `checkpoint-*.json`, `*-review-*.md`).

## Burn-down manifest, run 3 (21e49642), 2026-11-24 23:05 UTC

> Grant: "proceed", after run 2 left FUNC-1 for a run of its own. Then a yes to "Fix the build break first (a new run: the fix, then FUNC-1)", once `main` was found not to compile for the Mac or the iPad. Contract r3: STAB-1, then FUNC-1. Default limits. The same PR; no merge or deploy.
> Mode: bounded and owner-driven. Executor: Claude Opus 5.5 (`claude-opus-5.5`). Reviewers: read-only `code-review` agents asked for `gpt-6.2-astra`, which the host's status reports.
> Native gate: Xcode 27.2's toolchain, by `DEVELOPER_DIR`. This Mac's `xcode-select` points at the Command Line Tools, whose Swift lacks SwiftUI's macro plugins. Neither the repo nor CI configures a Swift linter or formatter, so for the Swift files the lint check is that the files changed, recompiled, draw no compiler warning, and the formatter check is the repo's `cargo fmt --check`.

| Item | State | Reworks | Commit | Acceptance evidence | Review |
|---|---|---|---|---|---|
| STAB-1 | pr-open | 0 | `79e9f1f14` | Before the fix, ScaenaKit failed at `Panels.swift:133`: "member operator '==' must have at least one argument of type 'ParsedTheme'". After it, `swift build --build-tests` with CI's linker flags builds the kit, the remote, the notes, the app, and the tests, and all 280 ScaenaKit tests pass. | approved; it notes no test pins that `ThemeText` skips its JSON |
| FUNC-1 | blocked | 2 | WIP at `refs/wip/lift-up-20261124-FUNC-1`, not committed | Before the change, the C ABI answered "`walking` is not a call this library answers" (its fallback arm). Now FFI's `code_is_walked_through_as_the_browser_walks_it` and ScaenaKit's `aWalkThroughAddsAStepForEachRange` pass: a walk adds a step for each set of lines after the first, lines are set in focus, Clear Focus works, a step past the last line is refused, and undo restores the deck. `scaena-ffi`'s 65 tests and all 281 ScaenaKit tests pass. Clippy, fmt, and the changed Swift files recompiled with no warning. | rejected twice. The first review found Focus on These Lines unreachable while typing, a walk that could apply after the deck changed, locks bypassed, CR/CRLF line counting, empty steps dropped, and an empty patch still made. The second found two left: a line kept in gutter numbers focuses the wrong one once the block is renumbered, and typing prose after code leaves the code's old lines offered. To finish: keep the touched lines as paragraph indices, add `code.from` when the command runs, and remember the last text typed whatever it is |

Shipped by run 3: STAB-1, in PR #1360, not merged. FUNC-1's acceptance tests pass at its WIP, but it was not accepted. No CI has run on the PR: the repo's last Actions run was on 2026-11-22. Deployment: none. PLAN 2.140 stays unchecked, since its `window.sh` code-walk shot was not made here.
