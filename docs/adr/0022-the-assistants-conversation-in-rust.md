# ADR-0022: The assistant's conversation in Rust, its calls made by the client

**Status:** proposed · **Date:** 2026-10-08

## Context

PLAN 3.6 asks for the user's own-key assistant on the Mac, as the browser's editor has it (PLAN 2.6, ADR-0011), and on-device models for small tasks (SPEC §11).

The browser's assistant is TypeScript (`web/src/assistant/`), about 850 lines besides its panel:
- **The providers.** Each adapter turns one conversation, kept in no provider's form, into its provider's request, and its answer back:
  - Anthropic's Messages API, its system prompt cached;
  - OpenAI's Chat Completions, a frame a tool drew sent as the user's image;
  - Gemini's generateContent, its thought signatures sent back as they came.
- **The loop.** A turn, the tools the model called run on the bundle, their results, the next turn, until the model answers or a question's rounds run out.
- **What the model is told.** The system prompt, and the tools: the MCP server's schemas from `docs/schema/mcp/`, less `bundle`, `out`, and `painter`.

The tools themselves run in Rust: the session's `tool` (ADR-0021).

A Swift port would be a second copy of each provider's details, and the two would drift. Those details are what breaks first: a thought signature dropped, an image in the wrong role, a tool's schema with a `$ref` a provider refuses.

HTTP cannot move into Rust:
- A page's module has no synchronous network.
- The Mac app should call through `URLSession`, under its sandbox and the system's proxies, with the key from the Keychain.

## Decision

1. **A crate of its own, `scaena-chat`, keeps the conversation.** It has:
   - the conversation, in no provider's form;
   - each provider's request built from it, and its answer read back;
   - the system prompt and the tools, from `scaena-resources` and the MCP tools' schemas;
   - the loop, as steps the client drives: an HTTP request for the client to make, the calls the session runs, or the answer.

   It makes no HTTP call, reads no clock, and keeps no key. A key handed in builds one request's headers and is gone with them.
2. **The FFI wraps it for Swift**, as it wraps the session. ScaenaKit makes each request with `URLSession` and hands the response back. The calls run on the session, by `agent:` and the model's name, as in the browser.
3. **Keys live in the Keychain** (ADR-0006), one item per provider, under the app's own service. A key goes to its provider and nowhere else, and is never written into a bundle.
4. **The browser keeps its TypeScript for now.** Moving the page onto `scaena-chat`, through the resources module its assistant already loads, is a later task. Until then the two are held together by one set of recorded exchanges: each provider's request for a conversation, and its answer read back. Each implementation's tests read them.
5. **On-device tasks use Apple's Foundation Models** where the Mac has them (macOS 26 and Apple Intelligence). They do three things, none of which edits without the user:
   - **Lint, explained.** What a finding means for this deck, in plain words.
   - **Notes, drafted.** A state's notes drafted from how it reads.
   - **Copy, tightened.** A text's words tightened, offered as a `replace_text` the user takes or leaves.

   Each sends nothing off the Mac. Where the models are absent, the actions are not offered.

## Consequences

- **+** One conversation for the Mac, tested in Rust here and by Swift on CI's Mac. The browser can take it on later without a third copy.
- **+** The key never crosses into Rust's keeping, nor into a bundle.
- **−** Until the browser moves, the providers' details live in TypeScript and Rust both. The recorded exchanges catch a drift, but do not prevent one.
- **−** One more crate, which the browser's modules do not carry yet.
