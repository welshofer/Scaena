// What the Mac app's window holds, as its accessibility tree says (PLAN 3.27): each element's role,
// identifier, name, and frame in screen points from the top left, to a depth; the frame of the
// first element with an identifier, as "x y width height"; what that element holds, each by its
// role and name, one to a line: the canvas's objects as a reader hears them; each split view in
// the window with the frame of each pane in it: how the window's width is shared out; or the value
// of the first element with an identifier, as a field shows what it holds (PLAN 3.28) and the hint
// over the canvas says what it does.
//
//   osascript -l JavaScript apps/mac/frames.js tree DEPTH
//   osascript -l JavaScript apps/mac/frames.js find IDENTIFIER
//   osascript -l JavaScript apps/mac/frames.js value IDENTIFIER
//   osascript -l JavaScript apps/mac/frames.js holds IDENTIFIER
//   osascript -l JavaScript apps/mac/frames.js splits
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

  const name = (e) => read(() => e.title()) || read(() => e.description()) || "";
  // The first element identified as `tag`: breadth first, as far as a window's chrome goes and no
  // further.
  const found = (tag) => {
    let level = [window];
    for (let depth = 0, seen = 0; depth < 16 && level.length && seen < 4000; depth++) {
      const next = [];
      for (const e of level) {
        seen++;
        if (id(e) === tag) return e;
        next.push(...children(e));
      }
      level = next;
    }
    throw new Error(`nothing is identified as ${tag}`);
  };
  if (what === "find") return frame(found(arg)).join(" ");
  if (what === "value") {
    const e = read(() => found(arg));
    return e ? String(read(() => e.value()) || name(e) || "") : "";
  }
  if (what === "holds") {
    return children(found(arg))
      .map((e) => `${read(() => e.role()) ?? "?"} "${name(e)}" ${frame(e).join(" ")}`)
      .join("\n");
  }
  // Breadth first through the window's groups, past what holds no split view: the toolbar, a
  // list, a scroll view, the canvas's objects, and every control.
  const into = new Set(["AXWindow", "AXGroup", "AXSplitGroup", "AXLayoutArea", "AXTabGroup"]);
  if (what === "splits") {
    const said = (e) => `${read(() => e.role()) ?? "?"}${id(e) ? ` #${id(e)}` : ""} ${frame(e).join(" ")}`;
    const lines = [];
    let level = [window];
    for (let depth = 0, seen = 0; depth < 16 && level.length && seen < 2000; depth++) {
      const next = [];
      for (const e of level) {
        seen++;
        const role = read(() => e.role());
        if (!into.has(role) || id(e) === "canvas") continue;
        const all = children(e);
        if (role === "AXSplitGroup") {
          lines.push(`${said(e)}:`, ...all.map((c) => `  ${said(c)}`));
        }
        next.push(...all);
      }
      level = next;
    }
    return lines.join("\n") || "no split views";
  }

  // Each element on a line of its own, indented by its depth; a long list cut short.
  const lines = [];
  const walk = (e, depth, most) => {
    const named = name(e);
    const tag = id(e);
    lines.push(
      `${"  ".repeat(depth)}${read(() => e.role()) ?? "?"}${tag ? ` #${tag}` : ""}${named ? ` "${named}"` : ""} ${frame(e).join(" ")}`,
    );
    if (depth >= most) return;
    const all = children(e);
    for (const c of all.slice(0, 24)) walk(c, depth + 1, most);
    if (all.length > 24) lines.push(`${"  ".repeat(depth + 1)}… and ${all.length - 24} more`);
  };
  walk(window, 0, Number(arg ?? 3));
  return lines.join("\n");
}
