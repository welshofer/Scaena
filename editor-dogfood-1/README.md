# Editor dogfood pass 1: the worst three screens

- `19-player.png`: the player opened from the editor shows "A new version is ready: reload" with no new version (every hidden control in the bar showed).
- `06-story.png`: the Story calls slide 4 "the close" while slide 5 ends the deck.
- `18-rehearsed.png`: the rehearsal's table opens with Discard in focus, so Return throws the rehearsal away with no undo.

Taken at 2x in headless Chromium with a scripted provider, from main at fce3e91.
