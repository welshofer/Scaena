# ADR-0001: One Rust engine producing a painter-agnostic display list

**Status:** accepted · **Date:** 2026-10-01

## Context

The product needs identical output in the browser, in native clients, and in headless export (PDF, video), with precise typography and deterministic frames. The two realistic architectures were:

1. **Browser-as-renderer**: HTML/CSS/WebGL in the web app, wrapped in Tauri/Electron for desktop; PDF/video via a headless Chromium. Fastest to a v1.
2. **Own engine**: a Rust core (layout, text shaping, animation sampling) that emits a serializable display list; thin painters per surface (WebGPU, Metal, CPU, PDF).

## Decision

Option 2. The engine is a pure function `render(document, theme, state, t, viewport) → DisplayList`. Painters never shape text or lay out; glyph positions arrive final in the display list.

## Consequences

- **+** Pixel parity across surfaces; deterministic video frames; PDF with real vector text and embedded subsets.
- **+** Golden tests on display lists are exact and cheap; raster tests are a second line.
- **+** Typography is ours: `balance`/`pretty` wrapping, cap-height alignment, hanging punctuation, variable axes.
- **−** Slower start; Phase 0 must prove text parity before anything else is built.
- **−** We own font loading, fallback, and subsetting.
- Fallback if Phase 0 fails: revisit option 1 with Remotion-style frame stepping for video. Recorded here so the choice is not relitigated casually.
