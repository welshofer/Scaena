Run the Phase 0 spike loop for Scaena.

1. Read `docs/PLAN.md` §Phase 0 and `docs/SPEC.md` §5, §6, §13. Identify the lowest unchecked 0.x task.
2. Before writing code, state in two sentences what the task proves and what "done" looks like (the exit criterion it feeds).
3. Implement it in the smallest crate that owns it (see CLAUDE.md repo map). Keep `scaena-core` free of fonts, filesystem, and clocks.
4. Add or update tests: display-list goldens under `tests/golden/`, raster goldens with tolerance, parity cases per torture-deck state.
5. Run `just check`. Fix everything. Record measured sizes/timings in `docs/spike-report.md` under the task's heading.
6. Tick the task in `docs/PLAN.md`, commit with a one-line imperative message referencing `PLAN 0.x`, and report: what was proved, what was measured, what is next.

If a kill criterion fails, do not lower the bar: write the failure into `docs/spike-report.md`, reference the no-go path in PLAN §0, and stop for a decision.

$ARGUMENTS
