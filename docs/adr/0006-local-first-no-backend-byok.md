# ADR-0006: Local-first, no backend; BYOK for LLMs

**Status:** accepted · **Date:** 2026-10-01

## Context

A backend costs money and operational attention and is not required by any v1 feature. LLM features must be available without the project paying for inference or holding user keys on a server.

## Decision

- A deck is a self-contained bundle on disk (SPEC §3.1). Storage is the filesystem (Mac), OPFS / File System Access (web), and whatever sync the user already has (iCloud, Dropbox, git).
- Sharing without a server: single-file HTML export (engine + bundle inlined, no network at runtime) and static hosting of the player.
- LLM integration is tiered: (1) any agent via MCP, no keys; (2) in-app assistant with the user's own keys, called directly from the client (Anthropic's browser opt-in header; OpenAI; Gemini), keys in Keychain or session/encrypted local storage; (3) on-device models on Mac.
- No telemetry by default. Keys are never written into bundles.

## Consequences

- **+** Zero infrastructure until Phase 5; nothing in v1 may assume a server.
- **+** Decks are portable artifacts users own.
- **−** Browser-stored keys carry risk; the UI must say so and offer session-only storage.
- **−** No link previews or hosted rendering until Phase 5 (reuses the CLI when it arrives).
