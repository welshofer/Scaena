// What the Mac app's window holds, as its accessibility tree says (PLAN 3.27): each element's role,
// identifier, name, and frame in screen points from the top left, to a depth; or the frame of the
// first element with an identifier, as "x y width height".
//
//   osascript -l JavaScript apps/mac/frames.js tree DEPTH
//   osascript -l JavaScript apps/mac/frames.js find IDENTIFIER
function run(argv) {
  const [what, arg] = argv;
  const window = Application("System Events").processes.byName("Scaena").windows[0];
  // An attribute an element may not have: none where it has not.
  const read = (f) => {
    try {
      return f();
    } catch (_) {
      return null;
    }
  };
  const id = (e) => read(() => e.attributes.byName("AXIdentifier").value());
  const frame = (e) => {
    const [x, y] = read(() => e.position()) ?? [0, 0];
    const [w, h] = read(() => e.size()) ?? [0, 0];
    return [x, y, w, h].map(Math.round);
  };
  const children = (e) => read(() => e.uiElements()) ?? [];

  if (what === "find") {
    // Breadth first, as far as a window's chrome goes and no further.
    let level = [window];
    for (let depth = 0, seen = 0; depth < 16 && level.length && seen < 4000; depth++) {
      const next = [];
      for (const e of level) {
        seen++;
        if (id(e) === arg) return frame(e).join(" ");
        next.push(...children(e));
      }
      level = next;
    }
    throw new Error(`nothing is identified as ${arg}`);
  }

  // Each element on a line of its own, indented by its depth; a long list cut short.
  const lines = [];
  const walk = (e, depth, most) => {
    const name = read(() => e.title()) || read(() => e.description()) || "";
    const tag = id(e);
    lines.push(
      `${"  ".repeat(depth)}${read(() => e.role()) ?? "?"}${tag ? ` #${tag}` : ""}${name ? ` "${name}"` : ""} ${frame(e).join(" ")}`,
    );
    if (depth >= most) return;
    const all = children(e);
    for (const c of all.slice(0, 24)) walk(c, depth + 1, most);
    if (all.length > 24) lines.push(`${"  ".repeat(depth + 1)}… and ${all.length - 24} more`);
  };
  walk(window, 0, Number(arg ?? 3));
  return lines.join("\n");
}
