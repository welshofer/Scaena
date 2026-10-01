# ADR-0003: The agent is the first user — CLI and MCP before any UI

**Status:** accepted · **Date:** 2026-10-01

## Context

Agents (Claude Code and MCP-capable tools) will author and maintain most decks. A UI is expensive and would otherwise be built first, with the agent surface bolted on. The LLM-integration requirement (BYOK, no backend) is also most cheaply satisfied by letting the agent bring its own model.

## Decision

The first client is the `scaena` CLI and an MCP server exposing the same operations (SPEC §7). Everything a UI can do must be reachable through them with machine-readable results. Lint findings carry fixes as patches; `deck_render` returns images so an agent can inspect its own work. Skills (`skills/*/SKILL.md`) encode authoring procedures and are versioned with the format.

## Consequences

- **+** Real decks exist in month one with zero UI; the agent loop (create → lint → render → patch) is tested in CI.
- **+** The web and Mac clients are thin consumers of the same operations; in-app assistants call the same functions.
- **+** Tier-1 LLM integration costs nothing and handles no keys.
- **−** Early UX is "source + preview," not direct manipulation. Accepted; direct manipulation emits patches later.
