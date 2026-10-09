# ADR-0025: The presenter remote joins over the local network with a code the editor shows

**Status:** proposed · **Date:** 2026-10-09

## Context

PLAN 4.10 and SPEC §9.5 ask for a remote. An iPhone or an iPad drives a deck that a Mac or an iPad presents, over the local network. It shows the notes and the next state.

A place in the deck is a state and a time into its cue (SPEC §2.4). That is all a show needs to tell a remote, and all a remote needs to ask of a show.

Four things are open:
- **How a remote finds the presenter.** Nobody types an address in a meeting room.
- **Who may drive the show.** Anyone on the same network can reach the presenter, a conference's Wi-Fi included.
- **What a remote draws.** It holds no deck, and an iPhone should not carry the engine to draw one.
- **Where the pairing happens.** On one display, the stage covers the whole screen, and the audience sees everything shown there.

## Decision

1. **Bonjour finds the presenter.**
   - Remote… in the Play menu turns the host on. While it is on, the Mac or the iPad offers its deck as `_scaena-remote._tcp`, under its own name and the deck's.
   - A remote lists the presenters on its network, Apple's peer-to-peer Wi-Fi included.
2. **A code the editor shows lets a remote in.** Turning Remote… on makes a code of four digits. The sheet shows it in the editor, never on the stage, so the audience does not see it.
   - Both ends derive TLS's pre-shared key from the code: HMAC-SHA256 of the protocol's name, keyed with the code. This is Apple's pattern for a peer-to-peer protocol.
   - The connection is TLS 1.2, with `TLS_PSK_WITH_AES_128_GCM_SHA256`, the cipher suite of Apple's pattern, which is TLS 1.2's.
   - A remote without the code fails the handshake, so it is never joined. Nothing passes in the clear.
   - Four digits are what a presentation app's remote pairs with. They guard a show against a passer-by, not against a determined attacker on the same network who records a handshake. That is enough for a slide show, and the code changes each time Remote… is turned on.
3. **Messages are JSON, each after its length.** Each frame is four bytes big-endian, then the JSON (`ScaenaRemote`'s `RemoteMessage`).
   - The presenter sends a place: the state, its index and the count, the time into its cue or that it is at rest, the notes, and when the show began.
   - It sends the state and the next drawn at rest, as PNGs from the engine's CPU painter. The remote draws only what it is sent.
   - A place is sent when the state changes or its cue comes to rest, not each frame.
   - A remote sends a command: play (from the slide the editor shows), on, back, first, last, and end. These are the player's own steps (PLAN 2.2).
4. **The remote holds no engine.** `ScaenaRemote` is a library of the ScaenaKit package that does not link `scaena-ffi`.
   - The iPhone's app (`apps/ipad/Remote`, built by the iPad's project) is it and a window.
   - The iPad app opens the same view as a window of its own: Control a Presentation… in the Play menu.
   - The Mac and the iPad present. Their host (`RemoteHost`) follows whichever show plays.

## Consequences

- One protocol, testable without a network. ScaenaKit's tests join a server on the loopback with the code, and fail to with another, on the Mac and on the iPad's simulator.
- The local network privacy prompt appears the first time Remote… is turned on, and the first time a remote looks. Each app's Info.plist names the service and says why.
- The remote shows the slides as pictures, so a cue's motion is not seen on it. It does not need to be: the audience watches the stage, and the remote shows where the show is.
- A remote that loses Wi-Fi mid-show says so and can join again with the same code, while the host stays on.
- A Mac as the remote, or one remote driving several presenters, can come later on the same messages.
