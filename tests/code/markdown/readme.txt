---
title: Field notes
tags: [trails, volunteers]
---

# Ridgeline trails

Volunteers rebuilt **fifty-two miles** of trail in *one* season: see the [report](https://example.org/report "2026 report") and [the map][map].

## Getting started

1. Install the CLI:
   ```bash
   cargo install scaena-cli --locked
   ```
2. Make a deck: `scaena new deck --theme dusk`
- [x] Fixed the bridge
- [ ] Mark the switchbacks

> **Note:** snake_case_names and 2 * 3 stay as written.

| Trail | Miles |
|:------|------:|
| Ridge | 12.5  |

```rust
fn main() {
    println!("{} miles", 52);
}
```

~~~
plain text, no language
~~~

Setext heading
--------------

<!-- a comment
over lines -->
![Trail map](assets/map.png) and <https://example.org>.

[map]: https://example.org/map "The map"
\*not emphasis\*
***
