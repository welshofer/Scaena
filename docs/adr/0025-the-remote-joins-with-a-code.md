# ADR-0025: The presenter remote joins over the local network with a code the editor shows

**Status:** accepted (Jay, 2026-10-11) · **Date:** 2026-10-09

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
   - No TLS session is resumed. A connection that resumed one another join made would skip the key, and join without the code; each join proves it again. A handshake with a pre-shared key costs little.
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

## Amendment: pairing by the code (lift-up SEC-5, 2026-10-23)

Decision 2 derived TLS's pre-shared key from the code alone. A handshake with a pre-shared key of four digits can be tried against offline: one join recorded on the network, and ten thousand keys, gives the code; and nothing limited tries online. The Bonjour name also told the room the deck's file name.

Now:
- **A password-authenticated key exchange pairs the ends**: SPEKE on P-256, with CryptoKit alone (`ScaenaRemote`'s `Pairing`). The code is hashed to a point of the curve: SHA-256 of a fixed label, the code, and a counter, the first x that is a point's, which every code of four digits reaches within 16 tries. The presenter works it out when it makes the code, so nothing a joining device can time depends on it. Each end raises it to a random secret and sends the result, which is a random point whatever the code, and raises the other's to its secret: the same key at both ends, never sent. A join recorded on the network tests no code offline, however many are tried against it. Every code's point is on the curve, whose order is prime, so a hello tells nothing of the code. (A first cut hashed the code to a u-coordinate on Curve25519, which lands on the curve for 4,979 codes and on its twist for 5,021; X25519 keeps a twist's point on the twist, so a hello told which half the code was in. P-256 replaced it.) Each end then proves it holds the key by a tag over both hellos, the remote first; a wrong code is one try, online. A point of low order, whose key anyone could make, is refused.
- **Every message after is sealed** under keys derived from the pairing, one each way (ChaCha20-Poly1305, `Sealed`), each numbered in its nonce and read in turn, so none is read, changed, replayed, or reordered on the wire. The connection is plain TCP; TLS is gone with its pre-shared key.
- **Tries are limited.** The presenter counts wrong codes (`RemoteServer.tried`). After five, it makes its code anew and says so (`recoded`), its sheet shows the new one, remotes still pairing are let go, and no remote pairs for five seconds, twice as long each time, up to five minutes, until a remote joins. A remote joined stays joined. A connection that speaks no pairing, or sends no proof, is no try, so a port scanner cannot change the code; at most sixteen pair at once. The sheet says how many wrong codes were tried, and that the code changed.
- **The Bonjour name is the device's alone**, within the 63 bytes a service's name may hold (`Remote.serviceName`). The deck's name reaches a remote only once it is joined, in each place.
- The code stays four digits, as a presentation app's remote pairs with, and is still shown only in the editor.

Not done, and open: the presenter approving each remote in the sheet before places and commands flow. With the pairing and the limit on tries it is defense in depth, and a call for Jay on how the sheet should read.

## Amendment: eight digits, and waits a join does not shorten (lift-up SEC-5, 2026-10-28)

The first amendment left two weaknesses. Four digits are ten thousand codes, and five tries at each made the code anew, so someone on the network, guessing slowly, found it with a chance of one in two thousand each time; and a remote joining set the waits back to five seconds, so the waits a guesser met stayed short while the presenter's remote came and went. The waits also grew to five minutes, long for a presenter whose own remote has to join again.

Now:
- **The code is eight digits**, a hundred million codes, drawn from the system's secure random numbers (`Remote.code`). The sheet shows it in two groups of four (`Remote.shown`); the remote's field takes digits alone, typed with the space or pasted (`Remote.typed`), and joins at eight. Five tries at a code are then one chance in twenty million; at a minute a code once the waits have doubled, a guesser needs decades on average. The pairing is unchanged, its version too: SPEKE hashes any code to its point, and the code's length is the presenter's and the remote's to agree on.
- **A join shortens no wait.** The waits are counted from when Remote… was turned on: five seconds, twice as long each time the code is made anew, up to a minute (`Remote.longest`), whether a remote joined between or not. A remote joined stays joined.
- **The presenter is never kept out for long.** Each wait is at most a minute, and the code made anew shows only in the editor, so whoever reads it there pairs once the wait is over; the sheet says to wait a moment. Someone on the network can still trip the waits, five wrong codes a minute, and so keep a remote not yet joined waiting, as anyone on a network can keep two devices on it apart. A remote joined before is untouched, and turning the remote off and on begins again. The presenter approving each remote in the sheet, still open, would close that too.
