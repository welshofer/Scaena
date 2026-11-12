// What the Mac app's window holds, as its accessibility tree says (PLAN 3.27): each element's role,
// identifier, name, and frame in screen points from the top left, to a depth; the frame of the
// first element with an identifier, as "x y width height"; what that element holds, each by its
// role and its title or description, one to a line (a label SwiftUI keeps as an attributed
// description reads as none here: the walk reads those with holds.swift); each split view in the window with the frame of each pane in it: how the window's width is
// shared out; or the value of the first element with an identifier, else what VoiceOver says it
// is, as a field shows what it holds (PLAN 3.28), the hint over the canvas says what it does, and
// the canvas names the slide it shows; or the button titled TITLE pressed, wherever the window, a
// sheet over it, or another window of the app holds it, as an alert's buttons may stand in a group
// of their own.
//
//   osascript -l JavaScript apps/mac/frames.js tree DEPTH
//   osascript -l JavaScript apps/mac/frames.js find IDENTIFIER
//   osascript -l JavaScript apps/mac/frames.js value IDENTIFIER
//   osascript -l JavaScript apps/mac/frames.js holds IDENTIFIER
//   osascript -l JavaScript apps/mac/frames.js splits
//   osascript -l JavaScript apps/mac/frames.js roots
//   osascript -l JavaScript apps/mac/frames.js press TITLE
function run(argv) {
  const [what, arg] = argv;
  const process = Application("System Events").processes.byName("Scaena");
  const window = process.windows[0];
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

  // What VoiceOver says an element is: its title, else its description (AXDescription, where an
  // AppKit control's label goes); none where it has neither. System Events cannot read the
  // attributed description SwiftUI keeps a label of its own in (holds.swift reads it), and its own
  // `description` falls back to the element's role ("group", "heading"), which says nothing of what
  // it holds.
  const label = (e) => read(() => e.title()) || read(() => e.attributes.byName("AXDescription").value()) || "";
  const name = (e) => label(e) || read(() => e.description()) || "";
  // The first element under `roots` that `matches`: breadth first, as far as a window's chrome
  // goes and no further; none where there is none.
  const search = (matches, roots) => {
    let level = roots;
    for (let depth = 0, seen = 0; depth < 16 && level.length && seen < 4000; depth++) {
      const next = [];
      for (const e of level) {
        seen++;
        if (matches(e)) return e;
        next.push(...children(e));
      }
      level = next;
    }
    return null;
  };
  // What the app shows at its top but its windows and its menu bar, where a popover may stand. The
  // windows are searched first, each with a budget of its own, and not again here (#1236).
  const besides = () =>
    (read(() => process.uiElements()) ?? []).filter((e) => !["AXMenuBar", "AXWindow"].includes(read(() => e.role())));
  // The popovers and sheets the window holds, as near its top as a toolbar's button: Add Slide's
  // picker stands there, under the window's thousands of elements, which took the budget before
  // the search came down to it (#1240's run: `roots` showed the window alone).
  const over = () => {
    const held = [];
    const role = (e) => read(() => e.role());
    // The window's own children, then down the toolbar alone, four deep: a few dozen elements.
    let level = children(window);
    for (let depth = 0; depth < 5 && level.length; depth++) {
      const next = [];
      for (const e of level) {
        const r = role(e);
        if (r === "AXPopover" || r === "AXSheet") held.push(e);
        else if (r === "AXToolbar" || depth > 0) next.push(...children(e));
      }
      level = next;
    }
    return held;
  };
  // The first element identified as `tag` in what stands over the window, then in the window; else
  // in the app's other windows, then in what else it shows but its menu bar.
  const found = (tag) => {
    const others = (read(() => process.windows()) ?? []).slice(1);
    const shown = besides();
    const is = (e) => id(e) === tag;
    const e = search(is, over()) ?? search(is, [window]) ?? search(is, others) ?? search(is, shown);
    if (!e) throw new Error(`nothing is identified as ${tag}`);
    return e;
  };
  if (what === "press") {
    const titled = (e) => read(() => e.role()) === "AXButton" && label(e) === arg;
    const others = (read(() => process.windows()) ?? []).slice(1);
    const e = search(titled, [window]) ?? search(titled, others);
    if (!e) throw new Error(`no button is titled ${arg}`);
    e.actions.byName("AXPress").perform();
    return "pressed";
  }
  if (what === "find") return frame(found(arg)).join(" ");
  // What the app shows at its top, one to a line: each window and each element beside them, then
  // each popover or sheet over the window, by its role, subrole, identifier, and name, its frame,
  // and how many elements it holds.
  if (what === "roots") {
    return [...(read(() => process.uiElements()) ?? []).filter((e) => read(() => e.role()) !== "AXMenuBar"), ...over()]
      .map((e) => {
        const sub = read(() => e.subrole());
        return `${read(() => e.role()) ?? "?"}${sub ? `/${sub}` : ""}${id(e) ? ` #${id(e)}` : ""}${name(e) ? ` "${name(e)}"` : ""} ${frame(e).join(" ")}, holding ${children(e).length}`;
      })
      .join("\n");
  }
  if (what === "value") {
    const e = read(() => found(arg));
    return e ? String(read(() => e.value()) || label(e) || "") : "";
  }
  if (what === "holds") {
    return children(found(arg))
      .map((e) => `${read(() => e.role()) ?? "?"} "${label(e)}" ${frame(e).join(" ")}`)
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
