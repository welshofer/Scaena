#!/usr/bin/env python3
"""The themes' library of layouts (PLAN 3.30, 3.31), written into each theme `docs/themes/registry.json` lists.

Each layout is a slide a person starts from Add Slide: slots on the theme's 12 × 12 grid, each
saying what goes in it. This script holds the library's catalog: the sections it is offered in,
in order, and every layout in each: the 47 the themes had, and those added since. It writes each
theme file's `layouts`, and nothing else in it.

- **The layouts are keyed by geometry family** (PLAN T.0a), as the registry names each theme's:
  Dusk's grid, which Daybreak and Salon share, or Ember's. A theme draws its family's layouts,
  and a new theme, whose file holds none, takes the older ones from its family's first file.
- **The layouts the themes had** keep the slots each family gave them: Ember's sit where its design
  wants them, and Dusk's family's are one. This script puts each in its section and gives
  the slots that wait for a picture or a figure an `example`, what the layout's picture shows there.
- **The layouts added since** are written here, once, on Dusk's grid: a headline in rows 1–2, what
  the slide holds in rows 3–10, and the kicker at the foot on the right. Ember's are made from
  them: its kicker runs along the top, its headline in rows 2–3, and what the slide holds a row
  lower, its large figures reaching the last row. A layout whose words would not fit Ember's larger
  figures says where they go.

Each slide a layout starts holds no more than 40 words, what the themes' density allows (W210), and
no two slots that hold words overlap. `crates/scaena-ops/tests/starting.rs` lints a slide started
in each layout in each theme.

    python3 scripts/build_layouts.py           # write the layouts into the theme files
    python3 scripts/build_layouts.py --check   # fail where a theme file is not what this writes
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "docs/themes/registry.json"


def themes(registry):
    """Each theme the registry lists, in its order: its slug, its geometry family, and its files.
    A theme's layouts are its family's (PLAN T.0a): Dusk's grid, which Daybreak and Salon share
    (PLAN 2.102), or Ember's."""
    return [(e["slug"], e["geometry"], [ROOT / f for f in e["files"]]) for e in registry["themes"]]

# The sections Add Slide offers the layouts in, in order.
SECTIONS = [
    "Titles",
    "Statements",
    "Lists",
    "Comparison",
    "Frameworks",
    "Process",
    "Time",
    "Numbers",
    "Charts",
    "Pictures",
    "People",
    "Editorial",
    "Diagrams",
    "Briefing",
    "Classroom",
    "Closing",
]

MAX_WORDS = 40
PICTURE = {"picture": "backdrop"}


def chart(kind):
    return {"chart": kind}


# ---------------------------------------------------------------------------------------------
# Slots, on Dusk's grid.


def slot(col, row, role=None, prompt=None, align=None, inset=None, bleed=None, example=None, fit=None):
    """A slot: its columns and rows, each a pair of tracks, first to last."""
    s = {"col": list(col), "row": list(row)}
    if align:
        s["align"] = align
    if role:
        s["role"] = role
    if inset:
        s["inset"] = inset
    if prompt is not None:
        s["prompt"] = prompt
    if bleed:
        s["bleed"] = True
    if example:
        s["example"] = example
    if fit:
        s["fit"] = fit
    return s


def card(col, row, fill="surface-2"):
    return slot(col, row, prompt={"shape": "rect", "fill": fill})


def ring(col, row, stroke="accent", shape="ellipse"):
    return slot(col, row, prompt={"shape": shape, "stroke": stroke})


def rule(col, row, points=None, shape="line", stroke=None):
    prompt = {"shape": shape}
    if stroke:
        prompt["stroke"] = stroke
    if points:
        prompt["points"] = points
    return slot(col, row, prompt=prompt)


def picture(col, row, words="A picture", bleed=None):
    return slot(col, row, prompt=words, bleed=bleed, example=PICTURE)


# The icons that ship (PLAN 2.104), as `scripts/build_icons.py` writes them.
ICON_SET = json.loads((ROOT / "docs/icons/phosphor.json").read_text())
ICON_PATHS = {i["name"]: i["regular"] for i in ICON_SET["icons"]}


def icon(col, row, name, inset=None, fill="accent"):
    """A place for an item's icon (PLAN G.23): a new slide puts the icon `name` there, from the set
    that ships, in its regular weight, for a person to swap for another; a deck made from a prompt
    puts there the icon its item's word names. The icon stands in the middle of the place, as large
    as it takes."""
    prompt = {"shape": "icon", "fill": fill, "icon": name, "path": ICON_PATHS[name], "view": ICON_SET["view"]}
    return slot(col, row, prompt=prompt, inset=inset)


KICKER = slot((9, 12), (11, 12), "kicker", align={"x": "end"})


def header(words="What this slide says", col=(1, 10)):
    return slot(col, (1, 2), "headline", words)


ORDINALS = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth"]
QUARTERS = [(1, 3), (4, 6), (7, 9), (10, 12)]
THIRDS = [(1, 4), (5, 8), (9, 12)]
HALVES = [(1, 6), (7, 12)]


# Where the middle of each column span stands, as a fraction of the grid's width: Dusk's, Daybreak's,
# and Ember's columns are alike (12 of them, 24 apart, 120 in from each side of a 1920 canvas).
def middle(col, of=(1, 12)):
    width, gutter, margin = 1920, 24, 120
    track = (width - 2 * margin - 11 * gutter) / 12
    left = lambda c: margin + (c - 1) * (track + gutter)
    right = lambda c: margin + c * track + (c - 1) * gutter
    x = (left(col[0]) + right(col[1])) / 2
    return round((x - left(of[0])) / (right(of[1]) - left(of[0])), 3)


# ---------------------------------------------------------------------------------------------
# The layouts added since the 47, by section. Each: its name, what it is for, and its slots, in the
# order a new slide places them: shapes first, under the words over them.

NEW = {}


def layout(section, name, description, slots, ember=None):
    assert name not in NEW, name
    NEW[name] = {"section": section, "description": description, "slots": slots, "ember": ember or {}}


# Titles ----------------------------------------------------------------------------------------

layout("Titles", "title-center", "A title centered on the slide, its subtitle under it: a cover with nothing else on it.", {
    "kicker": KICKER,
    "title": slot((2, 11), (4, 7), "display", "The talk's title", align={"x": "center", "y": "cap"}),
    "subtitle": slot((3, 10), (8, 9), "lede", "What it's about, in a line", align={"x": "center"}),
}, ember={"title": ((2, 11), (4, 8)), "subtitle": ((3, 10), (9, 11))})

layout("Titles", "title-split", "A picture off the left half of the slide, the title and its subtitle beside it.", {
    "art": picture((1, 6), (1, 12), "A picture, off the edge", bleed=True),
    "title": slot((8, 12), (3, 7), "headline", "The talk's title", align={"y": "end"}),
    "subtitle": slot((8, 12), (8, 9), "lede", "What it's about, in a line"),
    "kicker": KICKER,
}, ember={"title": ((8, 12), (4, 8)), "subtitle": ((8, 12), (9, 11)), "kicker": ((8, 12), 1)})

layout("Titles", "section-photo", "A section's opening beside a picture: its number, its name, and what it covers.", {
    "art": picture((8, 12), (1, 12), "A picture, off the edge", bleed=True),
    "number": slot((1, 4), (3, 4), "numeral", "02"),
    "title": slot((1, 6), (5, 8), "headline", "Section title", align={"y": "end"}),
    "support": slot((1, 6), (9, 10), "body", "What this part covers"),
}, ember={"number": ((1, 4), (2, 6)), "title": ((1, 6), (7, 10)), "support": ((1, 6), (11, 12))})

layout("Titles", "section-number", "A chapter's divider: its number, as large as the slide takes, beside its name.", {
    "kicker": KICKER,
    "number": slot((1, 6), (2, 9), "hero", "01", align={"y": "end"}),
    "title": slot((7, 12), (4, 7), "headline", "Section title", align={"y": "end"}),
    "support": slot((7, 12), (8, 9), "body", "What this part covers"),
}, ember={"number": ((1, 6), (2, 10)), "title": ((7, 12), (4, 8)), "support": ((7, 12), (9, 11))})

# Statements ------------------------------------------------------------------------------------

layout("Statements", "thesis", "One claim, centered and set large: the thought to leave the room with.", {
    "kicker": KICKER,
    "claim": slot((2, 11), (3, 8), "display", "One claim, set large", align={"x": "center", "y": "cap"}),
    "support": slot((3, 10), (9, 10), "lede", "Why it matters", align={"x": "center"}),
}, ember={"claim": ((2, 11), (3, 9)), "support": ((3, 10), (10, 12))})

layout("Statements", "big-question", "A question, set large, for the room to sit with, and why it's worth asking.", {
    "kicker": KICKER,
    "question": slot((1, 10), (3, 8), "display", "What would we make if we started today?", align={"x": "start", "y": "cap"}),
    "support": slot((1, 8), (9, 10), "lede", "Why it's worth asking"),
}, ember={"question": ((1, 10), (3, 9)), "support": ((1, 8), (10, 12))})

layout("Statements", "manifesto", "What you believe, one line to each belief, numbered: a credo.", {
    "kicker": KICKER,
    "header": header("What we believe"),
    "body": slot((1, 10), (3, 10), "lede", {"number": ["We build for the person in the room", "We keep it simple", "We show our work", "We finish what we start"]}),
})

layout("Statements", "key-point", "One point on its card, set large, and what follows from it.", {
    "kicker": KICKER,
    "card": card((2, 11), (3, 9)),
    "point": slot((2, 11), (3, 6), "headline", "The one point to remember", inset=48, align={"y": "end"}),
    "follow": slot((2, 11), (7, 9), "body", "What follows from it", inset=48),
}, ember={"card": ((2, 11), (3, 10)), "point": ((2, 11), (3, 7)), "follow": ((2, 11), (8, 10))})

layout("Statements", "haiku", "A haiku, set as written in the middle of the slide, and who wrote it or what it is on.", {
    "poem": slot((2, 11), (3, 8), "verse", "A haiku goes here\nseven syllables between\nfive more to close it", align={"x": "center", "y": "center"}, fit="shrink"),
    "rule": rule((6, 7), (9, 9)),
    "attribution": slot((4, 9), (10, 10), "caption", "Who wrote it, or what it is on", align={"x": "center"}),
}, ember={"poem": ((2, 11), (3, 9)), "rule": ((6, 7), (10, 10)), "attribution": ((4, 9), (11, 11))})

# Lists -----------------------------------------------------------------------------------------

layout("Lists", "bullets-photo", "A headline and the points that make its case, a picture off the right edge beside them.", {
    "art": picture((8, 12), (1, 12), "A picture, off the edge", bleed=True),
    "header": header(col=(1, 7)),
    "body": slot((1, 7), (3, 10), "lede", {"bullet": ["The first point", "The second point", "The third point"]}),
})

layout("Lists", "checklist", "What's done and what's left: each item on its row, its state at the end.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card((1, 12), r) for o, r in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)])},
    **{o: slot((1, 9), r, "body", w, inset=24) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["The first thing to do", "The second", "The third", "The fourth"])},
    **{f"{o}-state": slot((10, 12), r, "label", w, inset=24, align={"x": "end"}) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["Done", "Done", "Next", "Later"])},
})

layout("Lists", "agenda-photo", "What the talk covers, numbered, beside a picture off the left edge.", {
    "art": picture((1, 5), (1, 12), "A picture, off the edge", bleed=True),
    "header": slot((7, 12), (2, 3), "headline", "Agenda"),
    "body": slot((7, 12), (4, 10), "lede", {"number": ["The first topic", "The second topic", "The third topic", "The fourth topic"]}),
    "kicker": KICKER,
}, ember={"header": ((7, 12), (2, 3)), "body": ((7, 12), (4, 11)), "kicker": ((7, 12), 12)})

layout("Lists", "four-points", "Four points down the slide, each its number, its heading, and a line on it.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-number": slot((1, 1), r, "figure", str(i + 1)) for i, (o, r) in enumerate(zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)]))},
    **{f"{o}-title": slot((2, 6), r, "title", w) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["The first point", "The second", "The third", "The fourth"])},
    **{o: slot((7, 12), r, "body", "What it means") for o, r in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)])},
})

layout("Lists", "numbered-cards", "Four reasons, each on its card under its number.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, (3, 10)) for o, c in zip(ORDINALS, QUARTERS)},
    **{f"{o}-number": slot(c, (3, 5), "figure", f"0{i + 1}", inset=24, fit="grow") for i, (o, c) in enumerate(zip(ORDINALS, QUARTERS))},
    **{f"{o}-title": slot(c, (6, 7), "title", "A reason", inset=24, align={"y": "end"}) for o, c in zip(ORDINALS, QUARTERS)},
    **{o: slot(c, (8, 10), "body", "Why it counts, in a line", inset=24) for o, c in zip(ORDINALS, QUARTERS)},
})

# Icons (PLAN G.23) ------------------------------------------------------------------------------
# Each item under its icon, or beside it: features, values, reasons, or parts. An icon's place says
# which icon a new slide puts there, from the set that ships, for a person to swap for another; a
# deck made from a prompt puts there the icon its words name.

layout("Lists", "icon-columns", "Three things side by side, each under its icon and heading: features, values, or parts.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-icon": icon(c, (4, 5), i, inset=16) for o, c, i in zip(ORDINALS, THIRDS, ["lightning", "shield-check", "users-three"])},
    **{f"{o}-title": slot(c, (6, 7), "title", "A feature", align={"x": "center", "y": "center"}) for o, c in zip(ORDINALS, THIRDS)},
    **{o: slot(c, (8, 10), "body", "What it does, in a sentence or two.", align={"x": "center"}) for o, c in zip(ORDINALS, THIRDS)},
})

layout("Lists", "icon-cards", "Three cards side by side, each with its icon, its heading, and a few words: plans, promises, or parts.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, (3, 10)) for o, c in zip(ORDINALS, THIRDS)},
    **{f"{o}-icon": icon(c, (4, 5), i, inset=16) for o, c, i in zip(ORDINALS, THIRDS, ["rocket-launch", "target", "heart"])},
    **{f"{o}-title": slot(c, (6, 7), "title", "A promise", inset=24, align={"x": "center", "y": "center"}) for o, c in zip(ORDINALS, THIRDS)},
    **{o: slot(c, (8, 10), "body", "What it means, in a sentence or two.", inset=24, align={"x": "center"}) for o, c in zip(ORDINALS, THIRDS)},
})

FOUR_ROWS = [(3, 4), (5, 6), (7, 8), (9, 10)]

layout("Lists", "icon-rows", "Four points down the slide, each its icon, its heading, and a line on it.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-icon": icon((1, 1), r, i, inset=16) for o, r, i in zip(ORDINALS, FOUR_ROWS, ["compass", "chart-line-up", "handshake", "flag"])},
    **{f"{o}-title": slot((2, 6), r, "title", w, align={"y": "center"}) for o, r, w in zip(ORDINALS, FOUR_ROWS, ["The first point", "The second", "The third", "The fourth"])},
    **{o: slot((7, 12), r, "body", "What it means", align={"y": "center"}) for o, r in zip(ORDINALS, FOUR_ROWS)},
})

# Six in two rows of three: each icon in the first column of its third, its heading beside it, and
# a line under the heading.
GRID_SIX = [(c, top) for top in (3, 7) for c in THIRDS]

layout("Lists", "icon-grid", "Six features in two rows of three, each its icon beside its heading and a line under it.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-icon": icon((c[0], c[0]), (top, top + 1), i, inset=16) for o, (c, top), i in zip(ORDINALS, GRID_SIX, ["lightning", "lock", "globe", "clock", "chat-circle", "star"])},
    **{f"{o}-title": slot((c[0] + 1, c[1]), (top, top + 1), "title", "A feature", align={"y": "center"}) for o, (c, top) in zip(ORDINALS, GRID_SIX)},
    **{o: slot((c[0] + 1, c[1]), (top + 2, top + 3), "body", "What it does") for o, (c, top) in zip(ORDINALS, GRID_SIX)},
})

# Comparison ------------------------------------------------------------------------------------

layout("Comparison", "before-after", "How it was and how it is: two pictures, an arrow between them, each named.", {
    "kicker": KICKER,
    "header": header(),
    "first-art": picture((1, 5), (3, 8)),
    "arrow": rule((6, 7), (5, 6), shape="arrow"),
    "second-art": picture((8, 12), (3, 8)),
    "first": slot((1, 5), (9, 10), "title", "Before"),
    "second": slot((8, 12), (9, 10), "title", "After"),
})

layout("Comparison", "head-to-head", "Two options weighed point by point: each criterion in the middle, how each option does on either side.", {
    "kicker": KICKER,
    "header": header(),
    "middle-card": card((5, 8), (3, 10)),
    "first-title": slot((1, 4), (3, 4), "figure", "This one", align={"x": "center"}),
    "second-title": slot((9, 12), (3, 4), "figure", "That one", align={"x": "center"}),
    **{f"{o}-criterion": slot((5, 8), r, "label", w, align={"x": "center", "y": "center"}) for o, r, w in zip(ORDINALS, [(5, 6), (7, 8), (9, 10)], ["Cost", "Speed", "Risk"])},
    **{f"{o}-left": slot((1, 4), r, "body", w, align={"x": "center", "y": "center"}) for o, r, w in zip(ORDINALS, [(5, 6), (7, 8), (9, 10)], ["Lower", "Slower", "Known"])},
    **{f"{o}-right": slot((9, 12), r, "body", w, align={"x": "center", "y": "center"}) for o, r, w in zip(ORDINALS, [(5, 6), (7, 8), (9, 10)], ["Higher", "Faster", "New"])},
})

layout("Comparison", "scenarios", "Three ways it could go, side by side: the best case, the one expected, and the worst.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, (3, 10)) for o, c in zip(ORDINALS, THIRDS)},
    **{f"{o}-title": slot(c, (3, 4), "title", w, inset=32) for o, c, w in zip(ORDINALS, THIRDS, ["Best case", "Expected", "Worst case"])},
    **{f"{o}-figure": slot(c, (5, 6), "figure", w, inset=32) for o, c, w in zip(ORDINALS, THIRDS, ["+24%", "+12%", "+2%"])},
    **{o: slot(c, (7, 10), "body", "What would make it so", inset=32) for o, c in zip(ORDINALS, THIRDS)},
})

layout("Comparison", "myth-fact", "What people believe beside what is true: the myth, then the fact.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, (3, 10)) for o, c in zip(ORDINALS, HALVES)},
    "first-title": slot(HALVES[0], (3, 4), "label", "Myth", inset=32),
    "second-title": slot(HALVES[1], (3, 4), "label", "Fact", inset=32),
    "first": slot(HALVES[0], (5, 10), "lede", "What people believe", inset=32),
    "second": slot(HALVES[1], (5, 10), "lede", "What is true", inset=32),
})

layout("Comparison", "pricing", "Three plans side by side, each with its price and what it includes, the middle one marked.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, (3, 10)) for o, c in zip(ORDINALS, THIRDS)},
    "mark": rule(THIRDS[1], (3, 3), points=[[0, 0], [1, 0]], stroke="accent"),
    **{f"{o}-title": slot(c, (3, 4), "title", w, inset=32) for o, c, w in zip(ORDINALS, THIRDS, ["Basic", "Team", "Business"])},
    **{f"{o}-price": slot(c, (5, 6), "figure", w, inset=32) for o, c, w in zip(ORDINALS, THIRDS, ["$9", "$29", "$99"])},
    **{o: slot(c, (7, 10), "caption", {"bullet": ["What it includes", "And what else"]}, inset=32) for o, c in zip(ORDINALS, THIRDS)},
})

# Frameworks ------------------------------------------------------------------------------------

layout("Frameworks", "matrix", "A two-by-two with its axes named: where each thing falls, by two measures.", {
    "header": header(),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, [((3, 7), (3, 6)), ((8, 12), (3, 6)), ((3, 7), (7, 10)), ((8, 12), (7, 10))])},
    "y-high": slot((1, 2), (3, 6), "label", "More value", align={"y": "center"}),
    "y-low": slot((1, 2), (7, 10), "label", "Less value", align={"y": "center"}),
    **{o: slot(c, r, "title", w, inset=24, align={"x": "center", "y": "center"}) for o, (c, r), w in zip(ORDINALS, [((3, 7), (3, 6)), ((8, 12), (3, 6)), ((3, 7), (7, 10)), ((8, 12), (7, 10))], ["Quick wins", "Big bets", "Fill-ins", "Money pits"])},
    "x-low": slot((3, 7), (11, 11), "label", "Less effort", align={"x": "center"}),
    "x-high": slot((8, 12), (11, 11), "label", "More effort", align={"x": "center"}),
})

layout("Frameworks", "swot", "Strengths, weaknesses, opportunities, and threats, each a short list in its quarter.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))])},
    # The quarter's name stands on its points, set in the body's size in the rest of the card and
    # shrunk where there are more than it holds. A card's inset leaves a row of its own no room for
    # the name: it takes two, at their foot.
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "label", w, inset=24, align={"y": "end"}) for o, (c, r), w in zip(ORDINALS, [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))], ["Strengths", "Weaknesses", "Opportunities", "Threats"])},
    **{o: slot(c, (r[0] + 2, r[1]), "body", {"bullet": w}, inset=24, fit="shrink") for o, (c, r), w in zip(ORDINALS, [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))], [["What we do well", "Our edge"], ["Where we fall short", "What we lack"], ["What opens up", "Who needs us"], ["What could hurt us", "Who competes"]])},
})

layout("Frameworks", "okrs", "One objective, and the key results that say it is met, each with its target.", {
    "kicker": KICKER,
    "header": header("The objective, in a line"),
    **{f"{o}-card": card((1, 12), r) for o, r in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)])},
    **{o: slot((1, 9), r, "body", "A result you can measure", inset=24, align={"y": "center"}) for o, r in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)])},
    **{f"{o}-target": slot((10, 12), r, "figure", w, inset=24, align={"x": "end", "y": "center"}) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["80%", "12", "3×", "100%"])},
})

layout("Frameworks", "raci", "Who does what: the tasks down the side, the roles across, each cell responsible, accountable, consulted, or informed.", {
    "kicker": KICKER,
    "header": header(),
    "first-card": card((1, 12), (4, 5)),
    "third-card": card((1, 12), (8, 9)),
    **{f"{o}-role": slot(c, (3, 3), "label", w, align={"x": "center"}) for o, c, w in zip(ORDINALS, [(4, 6), (7, 9), (10, 12)], ["Lead", "Team", "Sponsor"])},
    **{f"{o}-task": slot((1, 3), r, "body", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, [(4, 5), (6, 7), (8, 9)], ["Plan it", "Do it", "Check it"])},
    **{f"{o}-{role}": slot(c, r, "title", w, align={"x": "center", "y": "center"}) for o, r, ws in zip(ORDINALS, [(4, 5), (6, 7), (8, 9)], [["A", "R", "I"], ["C", "R", "I"], ["A", "C", "R"]]) for role, c, w in zip(["lead", "team", "sponsor"], [(4, 6), (7, 9), (10, 12)], ws)},
})

layout("Frameworks", "three-horizons", "Bets across three horizons, near to far, each rising out of the one before.", {
    "kicker": KICKER,
    "header": header(),
    "first-card": card((1, 4), (7, 10)),
    "second-card": card((5, 8), (5, 10)),
    "third-card": card((9, 12), (3, 10)),
    "first-title": slot((1, 4), (7, 8), "title", "Defend the core", inset=24),
    "second-title": slot((5, 8), (5, 6), "title", "Grow what's next", inset=24),
    "third-title": slot((9, 12), (3, 4), "title", "Seed the future", inset=24),
    "first": slot((1, 4), (9, 10), "caption", "This year", inset=24),
    "second": slot((5, 8), (7, 8), "caption", "In two years", inset=24),
    "third": slot((9, 12), (5, 6), "caption", "In five years", inset=24),
})

layout("Frameworks", "venn", "Three circles that overlap: what each stands for, and what they share where they meet.", {
    "kicker": KICKER,
    "header": header(),
    "first-circle": ring((2, 5), (3, 9)),
    "second-circle": ring((5, 8), (3, 9)),
    "third-circle": ring((8, 11), (3, 9)),
    "first": slot((2, 4), (5, 6), "body", "Wanted", align={"x": "center", "y": "center"}),
    "second": slot((6, 7), (5, 6), "body", "Possible", align={"x": "center", "y": "center"}),
    "third": slot((9, 11), (5, 6), "body", "Paid for", align={"x": "center", "y": "center"}),
    "note": slot((3, 10), (10, 10), "caption", "Where all three meet is where to aim", align={"x": "center"}),
})

CYCLE = [((1, 5), (3, 5)), ((8, 12), (3, 5)), ((8, 12), (8, 10)), ((1, 5), (8, 10))]
layout("Frameworks", "cycle", "Four stages that come round again, each leading to the next.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, CYCLE)},
    "first-arrow": rule((6, 7), (4, 4), shape="arrow"),
    "second-arrow": rule((10, 10), (6, 7), points=[[0.5, 0], [0.5, 1]], shape="arrow"),
    "third-arrow": rule((6, 7), (9, 9), points=[[1, 0.5], [0, 0.5]], shape="arrow"),
    "fourth-arrow": rule((3, 3), (6, 7), points=[[0.5, 1], [0.5, 0]], shape="arrow"),
    **{o: slot(c, r, "title", w, inset=24, align={"x": "center", "y": "center"}) for o, (c, r), w in zip(ORDINALS, CYCLE, ["Plan", "Build", "Measure", "Learn"])},
})

NINE = [((c, r)) for r in [(3, 5), (6, 8), (9, 11)] for c in THIRDS]
layout("Frameworks", "nine-box", "Nine cells, three by three: where each falls on two scales at once.", {
    "header": header(),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, NINE)},
    **{o: slot(c, r, "title", w, inset=24, align={"x": "center", "y": "center"}) for o, (c, r), w in zip(ORDINALS, NINE, ["Rough diamond", "Future star", "Star", "Uneven", "Core player", "High performer", "At risk", "Effective", "Trusted pro"])},
})

layout("Frameworks", "stack", "Layers, each resting on the one under it: a platform, a product, a stack.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card((1, 12), r) for o, r in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)])},
    **{f"{o}-title": slot((1, 4), r, "title", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["Experience", "Services", "Platform", "Foundations"])},
    **{o: slot((5, 12), r, "body", "What it does", inset=24, align={"y": "center"}) for o, r in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)])},
})

layout("Frameworks", "hub-spoke", "One thing at the center and four around it, each joined to it.", {
    "kicker": KICKER,
    "header": header(),
    "hub-circle": slot((5, 8), (4, 9), prompt={"shape": "ellipse", "fill": "surface-2"}),
    "first-spoke": rule((4, 4), (4, 5), points=[[0, 0], [1, 1]]),
    "second-spoke": rule((9, 9), (4, 5), points=[[1, 0], [0, 1]]),
    "third-spoke": rule((4, 4), (8, 9), points=[[0, 1], [1, 0]]),
    "fourth-spoke": rule((9, 9), (8, 9), points=[[1, 1], [0, 0]]),
    "hub": slot((5, 8), (5, 8), "title", "What it all serves", align={"x": "center", "y": "center"}),
    **{f"{o}-title": slot(c, r, "title", "A part") for o, (c, r) in zip(ORDINALS, [((1, 3), (3, 4)), ((10, 12), (3, 4)), ((1, 3), (8, 9)), ((10, 12), (8, 9))])},
    **{o: slot(c, r, "caption", "What it gives") for o, (c, r) in zip(ORDINALS, [((1, 3), (5, 5)), ((10, 12), (5, 5)), ((1, 3), (10, 10)), ((10, 12), (10, 10))])},
})

# Process ---------------------------------------------------------------------------------------

CHEVRON_FIRST = [[0, 0], [0.86, 0], [1, 0.5], [0.86, 1], [0, 1]]
CHEVRON = [[0, 0], [0.86, 0], [1, 0.5], [0.86, 1], [0, 1], [0.14, 0.5]]
layout("Process", "process", "Four stages left to right, each pointing to the next, and what happens in each.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-chevron": slot(c, (3, 5), prompt={"shape": "polygon", "fill": "surface-2", "points": CHEVRON_FIRST if i == 0 else CHEVRON}) for i, (o, c) in enumerate(zip(ORDINALS, QUARTERS))},
    **{f"{o}-title": slot(c, (3, 5), "title", w, inset=24, align={"x": "center", "y": "center"}) for o, c, w in zip(ORDINALS, QUARTERS, ["Discover", "Define", "Build", "Launch"])},
    **{o: slot(c, (6, 10), "body", "What happens here") for o, c in zip(ORDINALS, QUARTERS)},
})

layout("Process", "how-it-works", "Three steps, each under its large number: how it works, start to finish.", {
    "kicker": KICKER,
    "header": header(),
    # A step reads as one piece: its number stands on its title, a row clear of the headline, and
    # its title on what happens. A title takes the two lines it needs at the set's size, every
    # step's words starting on one row; one too long for two shrinks, its set with it.
    **{f"{o}-number": slot(c, (4, 5), "numeral", str(i + 1), align={"y": "end"}) for i, (o, c) in enumerate(zip(ORDINALS, THIRDS))},
    **{f"{o}-title": slot(c, (6, 7), "title", w, fit="shrink") for o, c, w in zip(ORDINALS, THIRDS, ["Sign up", "Set it up", "Go"])},
    **{o: slot(c, (8, 10), "body", "What happens, in a line") for o, c in zip(ORDINALS, THIRDS)},
    # Ember's numeral is large enough that its descent, under a number set on its slot's foot,
    # reaches a row's gutter: its number ends a row above its title.
}, ember={**{f"{o}-number": (c, (4, 7)) for o, c in zip(ORDINALS, THIRDS)}, **{f"{o}-title": (c, (9, 10)) for o, c in zip(ORDINALS, THIRDS)}, **{o: (c, (11, 12)) for o, c in zip(ORDINALS, THIRDS)}})

layout("Process", "pipeline", "Stages in order, each with how many reach it and how many go on.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, (3, 9)) for o, c in zip(ORDINALS, QUARTERS)},
    **{f"{o}-title": slot(c, (3, 4), "title", w, inset=24) for o, c, w in zip(ORDINALS, QUARTERS, ["Leads", "Trials", "Customers", "Champions"])},
    **{f"{o}-figure": slot(c, (5, 6), "figure", w, inset=24) for o, c, w in zip(ORDINALS, QUARTERS, ["1,200", "340", "120", "36"])},
    **{o: slot(c, (7, 9), "caption", w, inset=24) for o, c, w in zip(ORDINALS, QUARTERS, ["28% go on", "35% go on", "30% go on", "Our best"])},
})

layout("Process", "journey", "A person's path in phases, and what they do, think, and feel at each.", {
    "kicker": KICKER,
    "header": header(),
    "second-lane": card((1, 12), (6, 7)),
    **{f"{o}-phase": slot(c, (3, 3), "label", w) for o, c, w in zip(ORDINALS, [(4, 6), (7, 9), (10, 12)], ["Discover", "Try", "Stay"])},
    **{f"{o}-lane": slot((1, 3), r, "label", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, [(4, 5), (6, 7), (8, 9)], ["Does", "Thinks", "Feels"])},
    # What they do, think, and feel are the slide's points: set in the body's size, shrunk where
    # one runs longer than its cell holds.
    **{f"{lane}-{o}": slot(c, r, "body", w, align={"y": "center"}, fit="shrink") for lane, r, ws in zip(["does", "thinks", "feels"], [(4, 5), (6, 7), (8, 9)], [["Hears of it", "Signs up", "Comes back"], ["Is it for me?", "This is easy", "I rely on it"], ["Curious", "Relieved", "At home"]]) for o, c, w in zip(ORDINALS, [(4, 6), (7, 9), (10, 12)], ws)},
})

layout("Process", "recipe", "What it takes and how it's done: what you need, then the method, step by step.", {
    "kicker": KICKER,
    "header": header(),
    "needs-title": slot((1, 4), (3, 4), "title", "You need"),
    "needs": slot((1, 4), (5, 10), "body", {"bullet": ["A team", "A plan", "A week"]}),
    "method-title": slot((6, 12), (3, 4), "title", "Method"),
    "method": slot((6, 12), (5, 10), "body", {"number": ["Agree on the goal", "Do the work", "Show what you made"]}),
})

# Time ------------------------------------------------------------------------------------------

layout("Time", "milestones", "What was reached when, down the slide: the dates on one side of a rule, what happened on the other.", {
    "kicker": KICKER,
    "header": header(),
    "rule": rule((3, 3), (3, 10), points=[[0.5, 0], [0.5, 1]]),
    **{f"{o}-date": slot((1, 2), r, "figure", w, align={"x": "end", "y": "center"}) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["2023", "2024", "2025", "2026"])},
    **{o: slot((4, 12), r, "body", "What was reached", align={"y": "center"}) for o, r in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)])},
})

layout("Time", "quarters", "The year by quarter, each a short list of what lands in it.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-title": slot(c, (3, 4), "figure", f"Q{i + 1}") for i, (o, c) in enumerate(zip(ORDINALS, QUARTERS))},
    "rule": rule((1, 12), (5, 5)),
    **{o: slot(c, (6, 10), "caption", {"bullet": ["A milestone", "Another"]}) for o, c in zip(ORDINALS, QUARTERS)},
})

layout("Time", "gantt", "Work streams across the months, each a bar where it runs.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-month": slot(c, (3, 3), "label", w) for o, c, w in zip(ORDINALS, [(4, 6), (7, 9), (10, 12)], ["April", "May", "June"])},
    **{f"{o}-bar": card(c, (r, r), fill="accent") for o, c, r in zip(ORDINALS, [(4, 7), (6, 10), (9, 12), (11, 12)], [4, 6, 8, 10])},
    **{o: slot((1, 3), (r, r), "body", w) for o, r, w in zip(ORDINALS, [4, 6, 8, 10], ["Design", "Build", "Launch", "Review"])},
})

layout("Time", "year", "Twelve months across the slide, the year's phases under them, and what each held.", {
    "kicker": KICKER,
    "header": header(),
    **{f"month-{i + 1}": slot((i + 1, i + 1), (3, 3), "label", m) for i, m in enumerate(["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"])},
    **{f"{o}-card": card(c, (4, 5)) for o, c in zip(ORDINALS, QUARTERS)},
    **{f"{o}-title": slot(c, (4, 5), "title", w, inset=24, align={"y": "center"}) for o, c, w in zip(ORDINALS, QUARTERS, ["Plan", "Build", "Launch", "Grow"])},
    **{o: slot(c, (6, 10), "caption", "What it held") for o, c in zip(ORDINALS, QUARTERS)},
})

layout("Time", "timeline-photos", "Four moments in order, each with a picture, its date, and what happened.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-art": picture(c, (3, 6)) for o, c in zip(ORDINALS, QUARTERS)},
    **{f"{o}-date": slot(c, (7, 8), "figure", w) for o, c, w in zip(ORDINALS, QUARTERS, ["2023", "2024", "2025", "2026"])},
    **{o: slot(c, (9, 10), "caption", "What happened") for o, c in zip(ORDINALS, QUARTERS)},
})

# Numbers ---------------------------------------------------------------------------------------

layout("Numbers", "two-figures", "Two figures side by side under the headline, each as large as the slide takes, with what it counts.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-figure": slot(c, (3, 6), "hero", w, align={"y": "end"}, fit="shrink") for o, c, w in zip(ORDINALS, [(1, 5), (7, 11)], ["$2.0T", "$1.1T"])},
    **{o: slot(c, (8, 9), "title", w) for o, c, w in zip(ORDINALS, [(1, 5), (7, 11)], ["What the first counts", "What the second counts"])},
    "note": slot((1, 8), (10, 10), "caption", "Its source"),
})

layout("Numbers", "four-numbers", "Four numbers in a square, each as large as its quarter of the slide takes, with what it counts and how it moved.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-figure": slot(c, r, "figure", w, align={"y": "end"}) for o, c, r, w in zip(ORDINALS, HALVES * 2, [(3, 4), (3, 4), (7, 8), (7, 8)], ["$4.2M", "38%", "12k", "2.1%"])},
    **{f"{o}-title": slot(c, (r, r), "body", w) for o, c, r, w in zip(ORDINALS, HALVES * 2, [5, 5, 9, 9], ["Revenue", "Margin", "Users", "Churn"])},
    **{o: slot(c, (r, r), "caption", "Up on last year") for o, c, r in zip(ORDINALS, HALVES * 2, [6, 6, 10, 10])},
})

layout("Numbers", "scorecard", "Four measures in a row, each its number and how it moved.", {
    "kicker": KICKER,
    "header": header(),
    "rule": rule((1, 12), (6, 6)),
    **{f"{o}-figure": slot(c, (3, 5), "figure", w, align={"y": "end"}) for o, c, w in zip(ORDINALS, QUARTERS, ["$4.2M", "38%", "12k", "2.1%"])},
    **{f"{o}-title": slot(c, (7, 8), "label", w) for o, c, w in zip(ORDINALS, QUARTERS, ["Revenue", "Margin", "Users", "Churn"])},
    **{o: slot(c, (9, 10), "caption", "Up on last year") for o, c in zip(ORDINALS, QUARTERS)},
})

layout("Numbers", "ranking", "The top four, in order, each with its number.", {
    "kicker": KICKER,
    "header": header(),
    "first-card": card((1, 12), (3, 4)),
    "third-card": card((1, 12), (7, 8)),
    **{f"{o}-rank": slot((1, 1), r, "figure", str(i + 1), inset=24, align={"y": "center"}) for i, (o, r) in enumerate(zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)]))},
    **{o: slot((2, 8), r, "title", w, align={"y": "center"}) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["The first", "The second", "The third", "The fourth"])},
    **{f"{o}-figure": slot((9, 12), r, "figure", w, inset=24, align={"x": "end", "y": "center"}) for o, r, w in zip(ORDINALS, [(3, 4), (5, 6), (7, 8), (9, 10)], ["$1.2M", "$980k", "$740k", "$510k"])},
})

layout("Numbers", "progress", "Three goals, each with a bar that shows how far it has come.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-track": card((5, 12), (r + 1, r + 1)) for o, r in zip(ORDINALS, [3, 6, 9])},
    **{f"{o}-bar": card((5, e), (r + 1, r + 1), fill="accent") for o, r, e in zip(ORDINALS, [3, 6, 9], [10, 8, 6])},
    **{o: slot((1, 4), (r, r + 1), "title", w, align={"y": "end"}) for o, r, w in zip(ORDINALS, [3, 6, 9], ["A goal", "Another", "A third"])},
    **{f"{o}-figure": slot((11, 12), (r, r), "value", w, align={"x": "end"}) for o, r, w in zip(ORDINALS, [3, 6, 9], ["75%", "50%", "25%"])},
})

layout("Numbers", "number-photo", "One number, large, what it means, and a picture off the right edge beside it.", {
    "art": picture((8, 12), (1, 12), "A picture, off the edge", bleed=True),
    "number": slot((1, 6), (2, 6), "hero", "42%", align={"y": "end"}, fit="shrink"),
    "claim": slot((1, 6), (8, 9), "title", "What the number means"),
    "under": slot((1, 6), (10, 10), "caption", "Its source"),
}, ember={"number": ((1, 6), (2, 7)), "claim": ((1, 6), (9, 10)), "under": ((1, 6), (11, 12))})

layout("Numbers", "metric-trend", "One number and the line that led to it: where it stands, and how it got there.", {
    "kicker": KICKER,
    "header": header(),
    "number": slot((1, 5), (3, 6), "hero", "42%", align={"y": "end"}, fit="shrink"),
    "claim": slot((1, 5), (8, 9), "title", "What the number means"),
    "main": slot((7, 12), (3, 8), prompt="A chart of how it got there", example=chart("line")),
    "note": slot((7, 12), (9, 9), "caption", "Six months, by month"),
    "source": slot((7, 12), (10, 10), "caption", "Its source"),
}, ember={"number": ((1, 5), (4, 8)), "claim": ((1, 5), (10, 12)), "main": ((7, 12), (4, 10)), "note": ((7, 12), (11, 11)),
          "source": ((7, 12), (12, 12))})

# Charts ----------------------------------------------------------------------------------------

layout("Charts", "bar-chart", "A headline, a bar chart that shows it, and the takeaway beside it.", {
    "kicker": KICKER,
    "header": header(),
    "main": slot((1, 8), (3, 10), prompt="A bar chart", example=chart("bar")),
    "takeaway": slot((9, 12), (3, 7), "lede", "The takeaway, in a sentence"),
    "note": slot((9, 12), (8, 9), "caption", "Its source"),
})

layout("Charts", "line-chart", "A headline over a line chart, the turn it takes called out beside it.", {
    "kicker": KICKER,
    "header": header(),
    "main": slot((1, 9), (3, 10), prompt="A line chart", example=chart("line")),
    "callout": slot((10, 12), (3, 5), "figure", "+38%", align={"y": "end"}),
    "note": slot((10, 12), (6, 8), "caption", "What changed, and when"),
    "source": slot((10, 12), (9, 10), "caption", "Its source"),
})

layout("Charts", "donut-chart", "Shares of a whole: a donut, and the parts that matter most beside it.", {
    "kicker": KICKER,
    "header": header(),
    "main": slot((1, 6), (3, 10), prompt="A donut chart", example=chart("donut")),
    "first-title": slot((8, 12), (3, 4), "figure", "46%"),
    "first": slot((8, 12), (5, 5), "caption", "The largest part"),
    "second-title": slot((8, 12), (7, 8), "figure", "31%"),
    "second": slot((8, 12), (9, 9), "caption", "The next"),
})

layout("Charts", "table", "A headline, a table that holds the facts, and a note on it.", {
    "kicker": KICKER,
    "header": header(),
    "main": slot((1, 12), (3, 10), prompt="A table", example={"table": 4}),
    "note": slot((1, 8), (11, 12), "caption", "Its source, or a note on it"),
})

layout("Charts", "chart-bullets", "A chart, and beside it the points it makes.", {
    "kicker": KICKER,
    "header": header(),
    "main": slot((1, 7), (3, 10), prompt="A chart", example=chart("stackedBar")),
    "body": slot((9, 12), (3, 10), "body", {"bullet": ["The first point", "The second", "The third"]}),
})

layout("Charts", "two-charts", "Two charts side by side, each under its heading.", {
    "kicker": KICKER,
    "header": header(),
    "first-title": slot(HALVES[0], (3, 4), "title", "The first"),
    "second-title": slot(HALVES[1], (3, 4), "title", "The second"),
    "first-chart": slot(HALVES[0], (5, 10), prompt="A chart", example=chart("bar")),
    "second-chart": slot(HALVES[1], (5, 10), prompt="A chart", example=chart("line")),
})

# Pictures --------------------------------------------------------------------------------------

layout("Pictures", "six-up", "Six pictures in a grid of three by two, under a headline.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-art": picture(c, r) for o, (c, r) in zip(ORDINALS, [(c, r) for r in [(3, 6), (7, 10)] for c in THIRDS])},
})

layout("Pictures", "panorama", "A picture across the width of the slide, its headline over it and a line under it.", {
    "kicker": KICKER,
    "header": header(),
    "art": picture((1, 12), (3, 8), "A wide picture", bleed=True),
    "note": slot((1, 8), (9, 10), "caption", "What it shows"),
})

layout("Pictures", "photo-quote", "A picture across the whole slide, and someone's words over it.", {
    "art": picture((1, 12), (1, 12), "A picture, edge to edge", bleed=True),
    "quote": slot((2, 9), (4, 8), "quote", "“The words someone said.”"),
    "who": slot((2, 9), (9, 10), "caption", "Who said it"),
}, ember={"quote": ((2, 9), (4, 9)), "who": ((2, 9), (10, 11))})

layout("Pictures", "four-in-a-row", "Four pictures in a row, each with its caption: a gallery.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-art": picture(c, (3, 7)) for o, c in zip(ORDINALS, QUARTERS)},
    **{o: slot(c, (8, 9), "caption", "What it shows") for o, c in zip(ORDINALS, QUARTERS)},
})

layout("Pictures", "callout", "A picture with notes around it, each pointing at a part of it.", {
    "kicker": KICKER,
    "header": header(),
    "art": picture((4, 9), (3, 10)),
    **{f"{o}-line": rule(c, (r, r)) for o, (c, r) in zip(ORDINALS, [((3, 4), 4), ((9, 10), 5), ((3, 4), 8), ((9, 10), 9)])},
    **{o: slot(c, (r, r + 1), "caption", "A note on this part") for o, (c, r) in zip(ORDINALS, [((1, 2), 3), ((11, 12), 4), ((1, 2), 7), ((11, 12), 8)])},
})

layout("Pictures", "photo-grid-text", "Four pictures to the left edge, a headline and its words beside them.", {
    **{f"{o}-art": picture(c, r, bleed=True) for o, (c, r) in zip(ORDINALS, [((1, 3), (1, 6)), ((4, 6), (1, 6)), ((1, 3), (7, 12)), ((4, 6), (7, 12))])},
    "header": slot((8, 12), (3, 5), "headline", "What this slide says"),
    "body": slot((8, 12), (6, 10), "body", "Words beside the pictures, a sentence or two."),
}, ember={"header": ((8, 12), (2, 4)), "body": ((8, 12), (5, 11))})

# People ----------------------------------------------------------------------------------------

layout("People", "bio", "One person: a picture, their name and what they do, and the arc of their work.", {
    "kicker": KICKER,
    "art": picture((1, 4), (3, 10), "A picture of them"),
    "name": slot((6, 12), (3, 4), "headline", "A name"),
    "role": slot((6, 12), (5, 5), "label", "What they do"),
    "story": slot((6, 12), (6, 10), "body", "The arc of their work, in two sentences."),
}, ember={"art": ((1, 4), (3, 11)), "name": ((6, 12), (3, 4)), "role": ((6, 12), (5, 5)), "story": ((6, 12), (6, 11))})

layout("People", "team-four", "Four people in a row, each with a picture, a name, and what they do.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-art": picture(c, (3, 6), "A picture of them") for o, c in zip(ORDINALS, QUARTERS)},
    **{f"{o}-title": slot(c, (7, 8), "title", "A name") for o, c in zip(ORDINALS, QUARTERS)},
    **{o: slot(c, (9, 9), "caption", "What they do") for o, c in zip(ORDINALS, QUARTERS)},
})

TEAM_SIX = [(c, r) for r in [(3, 5), (7, 9)] for c in THIRDS]
layout("People", "team-six", "Six people, three by two, each with a picture, a name, and what they do.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-art": picture((c[0], c[0] + 1), r, "A picture of them") for o, (c, r) in zip(ORDINALS, TEAM_SIX)},
    **{f"{o}-title": slot((c[0] + 2, c[1]), (r[0], r[0] + 1), "title", "A name") for o, (c, r) in zip(ORDINALS, TEAM_SIX)},
    **{o: slot((c[0] + 2, c[1]), (r[1], r[1]), "caption", "What they do") for o, (c, r) in zip(ORDINALS, TEAM_SIX)},
})

TEAM_NAMES = [(c, r) for r in [(3, 6), (7, 10)] for c in THIRDS]
layout("People", "team-names", "Up to six people, three by two, each on a card with their name and what they do: a team with no pictures of them.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, TEAM_NAMES)},
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "title", "A name", inset=24, align={"y": "end"}) for o, (c, r) in zip(ORDINALS, TEAM_NAMES)},
    **{o: slot(c, (r[0] + 2, r[1]), "body", "What they do", inset=24, fit="shrink") for o, (c, r) in zip(ORDINALS, TEAM_NAMES)},
})

ORG = [(1, 4), (5, 8), (9, 12)]
layout("People", "org-chart", "Who reports to whom: one at the top, and three under them.", {
    "kicker": KICKER,
    "header": header(),
    "top-card": card((5, 8), (3, 4)),
    **{f"{o}-card": card(c, (8, 9)) for o, c in zip(ORDINALS, ORG)},
    "trunk": rule((1, 12), (5, 7), points=[[0.5, 0], [0.5, 0.5]]),
    "branch": rule((1, 12), (5, 7), points=[[middle(ORG[0]), 0.5], [middle(ORG[2]), 0.5]]),
    **{f"{o}-drop": rule((1, 12), (5, 7), points=[[middle(c), 0.5], [middle(c), 1]]) for o, c in zip(ORDINALS, ORG)},
    "top": slot((5, 8), (3, 4), "title", "A leader", inset=24, align={"x": "center", "y": "center"}),
    **{o: slot(c, (8, 9), "title", "A name", inset=24, align={"x": "center", "y": "center"}) for o, c in zip(ORDINALS, ORG)},
})

layout("People", "speaker", "Who is speaking: their picture off the left edge, their name and what they do, and what they'll talk about.", {
    "art": picture((1, 5), (1, 12), "A picture of them", bleed=True),
    "name": slot((7, 12), (3, 4), "headline", "A name"),
    "role": slot((7, 12), (5, 5), "label", "What they do"),
    "talk": slot((7, 12), (7, 9), "lede", "What they'll talk about"),
}, ember={"name": ((7, 12), (3, 4)), "role": ((7, 12), (5, 5)), "talk": ((7, 12), (7, 10))})

layout("People", "two-quotes", "Two people's words side by side, each on its card with who said them.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-card": card(c, (3, 10)) for o, c in zip(ORDINALS, HALVES)},
    **{o: slot(c, (3, 8), "quote", "“What they said.”", inset=32) for o, c in zip(ORDINALS, HALVES)},
    **{f"{o}-note": slot(c, (9, 10), "caption", "Who said it", inset=32) for o, c in zip(ORDINALS, HALVES)},
})

# Editorial -------------------------------------------------------------------------------------

layout("Editorial", "letter", "A letter: who it's from, the date, who it's to, what it says, and who signs it.", {
    "kicker": KICKER,
    "from": slot((1, 6), (1, 2), "label", "From our team"),
    "date": slot((9, 12), (1, 2), "caption", "October 2026", align={"x": "end"}),
    "to": slot((1, 8), (3, 4), "title", "Dear friends,"),
    "body": slot((1, 9), (5, 9), "body", "What we want to say, in a few sentences."),
    "sign": slot((1, 6), (10, 11), "title", "With thanks, the team"),
}, ember={"from": ((1, 6), (2, 3)), "date": ((9, 12), (2, 3)), "to": ((1, 8), (4, 5)), "body": ((1, 9), (6, 10)), "sign": ((1, 6), (11, 12)), "kicker": ((1, 12), 1)})

layout("Editorial", "memo", "A memo: who it's to and from, what it's about, and when, then what it says.", {
    "kicker": KICKER,
    "header": header("Memo"),
    **{f"{k}-label": slot((1, 2), (r, r), "label", w) for k, r, w in zip(["to", "from", "about", "date"], [3, 4, 5, 6], ["To", "From", "About", "Date"])},
    **{k: slot((3, 9), (r, r), "body", w) for k, r, w in zip(["to", "from", "about", "date"], [3, 4, 5, 6], ["Everyone", "The team", "What it's about", "Today"])},
    "rule": rule((1, 12), (7, 7)),
    "body": slot((1, 10), (8, 10), "body", "What it says, in a sentence or two."),
})

RECEIPT = [3, 4, 5, 6]
layout("Editorial", "receipt", "What it cost, line by line, and the total, as a receipt sets it out.", {
    "kicker": KICKER,
    "slip": card((4, 9), (1, 10)),
    "title": slot((5, 8), (1, 2), "headline", "Receipt", align={"x": "center"}),
    **{f"{o}-item": slot((5, 7), (r, r), "body", w) for o, r, w in zip(ORDINALS, RECEIPT, ["An item", "Another", "A third", "A fourth"])},
    **{f"{o}-amount": slot((8, 8), (r, r), "body", w, align={"x": "end"}) for o, r, w in zip(ORDINALS, RECEIPT, ["$12", "$18", "$9", "$9"])},
    "rule": rule((5, 8), (7, 7)),
    "total-label": slot((5, 6), (8, 9), "title", "Total"),
    "total": slot((7, 8), (8, 9), "title", "$48", align={"x": "end"}),
}, ember={"slip": ((4, 9), (2, 12)), "title": ((5, 8), (2, 3)), **{f"{o}-item": ((5, 7), (r + 1, r + 1)) for o, r in zip(ORDINALS, RECEIPT)}, **{f"{o}-amount": ((8, 8), (r + 1, r + 1)) for o, r in zip(ORDINALS, RECEIPT)}, "rule": ((5, 8), (8, 8)), "total-label": ((5, 6), (9, 10)), "total": ((7, 8), (9, 10))})

layout("Editorial", "chyron", "A news lower third: a picture across the slide, and a band across its foot with the story.", {
    "art": picture((1, 12), (1, 12), "A picture, edge to edge", bleed=True),
    "band": card((1, 12), (9, 11)),
    "tag": slot((1, 4), (9, 9), "label", "Breaking"),
    "story": slot((1, 10), (10, 11), "title", "What happened, in a line"),
}, ember={"band": ((1, 12), (9, 12)), "tag": ((1, 4), (9, 9)), "story": ((1, 10), (10, 12))})

layout("Editorial", "headline", "A front page: the section, the headline, the line under it, the byline, and a picture.", {
    "tag": slot((1, 6), (1, 1), "label", "The section"),
    "headline": slot((1, 10), (2, 4), "headline", "The headline, in a line"),
    "dek": slot((1, 8), (5, 6), "lede", "What it means, in a sentence"),
    "byline": slot((1, 6), (7, 7), "label", "By a name"),
    "art": picture((1, 12), (8, 12), "A picture across the foot", bleed=True),
}, ember={"tag": ((1, 6), (1, 1)), "headline": ((1, 10), (2, 4)), "dek": ((1, 8), (5, 6)), "byline": ((1, 6), (7, 7)), "art": ((1, 12), (8, 12))})

FAQ = [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))]
layout("Editorial", "faq", "The questions people ask, each on its card with its answer.", {
    "kicker": KICKER,
    "header": header("What people ask"),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, FAQ)},
    # A question stands on its answer, however many lines it takes.
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "title", "A question?", inset=24, align={"y": "end"}) for o, (c, r) in zip(ORDINALS, FAQ)},
    **{o: slot(c, (r[0] + 2, r[1]), "body", "Its answer, in a line.", inset=24) for o, (c, r) in zip(ORDINALS, FAQ)},
})

GLOSSARY = [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))]
layout("Editorial", "glossary", "Terms and what they mean, set as a dictionary sets them.", {
    "kicker": KICKER,
    "header": header("Terms"),
    # A term stands on what it means, as a dictionary sets it.
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "title", "A term", align={"y": "end"}) for o, (c, r) in zip(ORDINALS, GLOSSARY)},
    **{o: slot(c, (r[0] + 2, r[1]), "body", "What it means, in a line.") for o, (c, r) in zip(ORDINALS, GLOSSARY)},
})

layout("Editorial", "magazine", "A spread: a headline, two columns of words, and a line pulled from them between.", {
    "kicker": KICKER,
    "header": header(),
    "first": slot((1, 4), (3, 10), "body", "Words for the first column, a sentence or two."),
    "pull": slot((5, 8), (4, 8), "quote", "“A line worth pulling.”"),
    "second": slot((9, 12), (3, 10), "body", "Words for the second column, a sentence or two."),
})

# Closing ---------------------------------------------------------------------------------------

layout("Closing", "next-steps", "What happens next: each action, who owns it, and by when.", {
    "kicker": KICKER,
    "header": header("Next steps"),
    "action-label": slot((1, 6), (3, 3), "label", "Action"),
    "owner-label": slot((7, 9), (3, 3), "label", "Owner"),
    "when-label": slot((10, 12), (3, 3), "label", "By"),
    **{f"{o}-card": card((1, 12), r) for o, r in zip(ORDINALS, [(4, 5), (8, 9)])},
    **{o: slot((1, 6), r, "body", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, [(4, 5), (6, 7), (8, 9)], ["The first thing to do", "The second", "The third"])},
    **{f"{o}-owner": slot((7, 9), r, "body", "A name", align={"y": "center"}) for o, r in zip(ORDINALS, [(4, 5), (6, 7), (8, 9)])},
    **{f"{o}-when": slot((10, 12), r, "body", w, align={"y": "center"}) for o, r, w in zip(ORDINALS, [(4, 5), (6, 7), (8, 9)], ["Friday", "Next week", "In March"])},
})

layout("Closing", "the-ask", "What you need from the room: one request, set large, and three things it takes.", {
    "kicker": KICKER,
    "ask": slot((1, 10), (3, 6), "display", "What we need from you", align={"y": "cap"}),
    **{o: slot(c, (8, 10), "body", w) for o, c, w in zip(ORDINALS, THIRDS, ["One thing it takes", "Another", "And a third"])},
}, ember={"ask": ((1, 10), (3, 7)), **{o: (c, (9, 11)) for o, c in zip(ORDINALS, THIRDS)}})

layout("Closing", "qa", "Questions, set large, and where to send the ones that come later.", {
    "kicker": KICKER,
    "question": slot((1, 12), (4, 7), "display", "Questions?", align={"x": "center", "y": "cap"}),
    "support": slot((3, 10), (9, 10), "lede", "Where to send the rest", align={"x": "center"}),
})

layout("Closing", "contact", "How to reach you: thanks, your name, where to find you, and a picture off the right edge.", {
    "art": picture((8, 12), (1, 12), "A picture, off the edge", bleed=True),
    "thanks": slot((1, 6), (3, 5), "display", "Thank you", align={"y": "cap"}),
    "name": slot((1, 6), (7, 8), "title", "Your name"),
    "details": slot((1, 6), (9, 10), "body", "Where to reach you"),
}, ember={"thanks": ((1, 6), (3, 6)), "name": ((1, 6), (8, 9)), "details": ((1, 6), (10, 11))})

layout("Closing", "takeaways", "What to remember: three takeaways, each on its card under its number.", {
    "kicker": KICKER,
    "header": header("What to remember"),
    **{f"{o}-card": card(c, (3, 10)) for o, c in zip(ORDINALS, THIRDS)},
    **{f"{o}-number": slot(c, (3, 4), "figure", str(i + 1), inset=32) for i, (o, c) in enumerate(zip(ORDINALS, THIRDS))},
    **{o: slot(c, (5, 10), "lede", "The thing to remember", inset=32) for o, c in zip(ORDINALS, THIRDS)},
})

layout("Closing", "call-to-action", "One thing to do next, set large, and a button that says how.", {
    "kicker": KICKER,
    "statement": slot((1, 10), (3, 7), "display", "One thing to do next", align={"y": "cap"}),
    "button-card": ring((1, 4), (9, 10), shape="rect"),
    "button": slot((1, 4), (9, 10), "title", "Start here", inset=24, align={"x": "center", "y": "center"}),
}, ember={"statement": ((1, 10), (3, 8)), "button-card": ((1, 4), (10, 11)), "button": ((1, 4), (10, 11))})

# ---------------------------------------------------------------------------------------------
# The first sixteen of ADR-0031's slide types (PLAN S.1), each drawn as its table says.

# Where a point of the grid stands in a slot, as a fraction of it, on Dusk's grid (Daybreak's and
# Salon's are alike, and Ember's rows near enough): what a line or an arrow's `points` give.
ROW, ROW_GUTTER, TOP = 52, 24, 96


def x_of(c, side):
    width, gutter, margin = 1920, 24, 120
    track = (width - 2 * margin - 11 * gutter) / 12
    return margin + (c - 1) * (track + gutter) + (track if side == "right" else 0)


def y_of(r, side):
    return TOP + (r - 1) * (ROW + ROW_GUTTER) + (ROW if side == "bottom" else 0)


def at(col, row, x, y):
    """The point `(x, y)`, canvas units, as a fraction of the slot in `col` and `row`."""
    left, right = x_of(col[0], "left"), x_of(col[1], "right")
    top, bottom = y_of(row[0], "top"), y_of(row[1], "bottom")
    return [round((x - left) / (right - left), 3), round((y - top) / (bottom - top), 3)]


def arrow(col, row, start, end, stroke="onSurfaceMuted"):
    """An arrow in `col` and `row` from `start` to `end`, each a point of the canvas."""
    return rule(col, row, points=[at(col, row, *start), at(col, row, *end)], shape="arrow", stroke=stroke)


def box(col, row, fill="surface-2", inset=None):
    """A card that stands in from its cells by `inset` on each side, as words on it do."""
    return slot(col, row, prompt={"shape": "rect", "fill": fill}, inset=inset)


DIAMOND = [[0.5, 0], [1, 0.5], [0.5, 1], [0, 0.5]]

# Numbers ---------------------------------------------------------------------------------------

layout("Numbers", "kpi-dashboard", "Four measures in a row, each its figure and how it moved, and the trends under them.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-figure": slot(c, (3, 4), "figure", w, align={"y": "end"}) for o, c, w in zip(ORDINALS, QUARTERS, ["$4.2M", "38%", "12k", "2.1%"])},
    **{f"{o}-title": slot(c, (5, 5), "label", w) for o, c, w in zip(ORDINALS, QUARTERS, ["Revenue", "Margin", "Users", "Churn"])},
    **{o: slot(c, (6, 6), "caption", w) for o, c, w in zip(ORDINALS, QUARTERS, ["Up 12%", "Up 3 points", "Up 9%", "Down 0.4 points"])},
    "first-chart": slot(HALVES[0], (7, 10), prompt="A chart of a trend", example=chart("line")),
    "second-chart": slot(HALVES[1], (7, 10), prompt="A chart of a trend", example=chart("bar")),
})

# Workstreams, each on its row: its name, its state's icon and word, what comes next, and its risk.
STREAMS = [(4, 5), (6, 7), (8, 9)]
layout("Numbers", "status-report", "Where each workstream stands: its status, its next milestone, and its risk.", {
    "kicker": KICKER,
    "header": header(),
    "first-card": card((1, 12), STREAMS[0]),
    "third-card": card((1, 12), STREAMS[2]),
    **{f"{k}-label": slot(c, (3, 3), "label", w) for k, c, w in zip(["stream", "status", "next", "risk"], [(1, 3), (4, 6), (7, 9), (10, 12)], ["Workstream", "Status", "Next milestone", "Risk"])},
    **{f"{o}-icon": icon((4, 4), r, i, inset=16, fill=f) for o, r, i, f in zip(ORDINALS, STREAMS, ["check-circle", "warning-circle", "x-circle"], ["accent", "accent-2", "onSurfaceMuted"])},
    **{o: slot((1, 3), r, "body", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, STREAMS, ["Design", "Build", "Launch"])},
    **{f"{o}-status": slot((5, 6), r, "body", w, align={"y": "center"}) for o, r, w in zip(ORDINALS, STREAMS, ["On track", "At risk", "Late"])},
    **{f"{o}-next": slot((7, 9), r, "body", w, align={"y": "center"}) for o, r, w in zip(ORDINALS, STREAMS, ["Review, May 2", "Beta, June 9", "Launch, July 1"])},
    **{f"{o}-risk": slot((10, 12), r, "caption", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, STREAMS, ["None", "Hiring", "Vendor delay"])},
})

# Three markets, each inside the last, set on one baseline: the circles as round as the grid's
# tracks make them, in their columns' middle; and each one's figure and name beside it, at its top.
MARKETS = [((2, 6), (3, 12)), ((3, 5), (7, 12)), ((4, 4), (11, 12))]
layout("Numbers", "market-size", "How large the market is: the total, the part we can serve, and the part we can win, each inside the last.", {
    "header": header(),
    "first-circle": slot(*MARKETS[0], prompt={"shape": "ellipse", "fill": "surface-2"}),
    "second-circle": slot(*MARKETS[1], prompt={"shape": "ellipse", "stroke": "accent"}),
    "third-circle": slot(*MARKETS[2], prompt={"shape": "ellipse", "fill": "accent"}),
    **{f"{o}-figure": slot((8, 9), (r, r + 1), "figure", w) for o, r, w in zip(ORDINALS, [3, 7, 11], ["$48B", "$12B", "$2B"])},
    **{f"{o}-title": slot((10, 12), (r, r), "label", w) for o, r, w in zip(ORDINALS, [3, 7, 11], ["Total market", "Serviceable", "Obtainable"])},
    **{o: slot((10, 12), (r + 1, r + 1), "caption", w) for o, r, w in zip(ORDINALS, [3, 7, 11], ["Everyone who could buy", "Those we can reach", "What we can win"])},
}, ember={
    "first-circle": ((2, 6), (4, 12)), "second-circle": ((3, 5), (7, 12)), "third-circle": ((4, 4), (11, 12)),
    **{f"{o}-figure": ((8, 9), (r, r + 1)) for o, r in zip(ORDINALS, [4, 7, 11])},
    **{f"{o}-title": ((10, 12), r) for o, r in zip(ORDINALS, [4, 7, 11])},
    **{o: ((10, 12), r + 1) for o, r in zip(ORDINALS, [4, 7, 11])},
})

# Diagrams --------------------------------------------------------------------------------------

layout("Diagrams", "flowchart", "Steps and a yes-or-no decision, joined by arrows: what happens, and what happens when it doesn't.", {
    "kicker": KICKER,
    "header": header(),
    "first-card": card((1, 2), (5, 6)),
    "first-arrow": arrow((3, 3), (5, 6), (x_of(3, "left"), y_of(5, "bottom") + 12), (x_of(3, "right"), y_of(5, "bottom") + 12)),
    "decision-shape": slot((4, 6), (4, 7), prompt={"shape": "polygon", "fill": "surface-2", "points": DIAMOND}),
    "yes-arrow": arrow((7, 7), (5, 6), (x_of(7, "left"), y_of(5, "bottom") + 12), (x_of(7, "right"), y_of(5, "bottom") + 12)),
    "second-card": card((8, 10), (5, 6)),
    "no-arrow": arrow((5, 5), (8, 8), (x_of(5, "left") + 67, y_of(8, "top")), (x_of(5, "left") + 67, y_of(8, "bottom"))),
    "third-card": card((4, 6), (9, 10)),
    "back-arrow": rule((1, 3), (7, 10), points=[
        at((1, 3), (7, 10), x_of(4, "left") - 12, y_of(9, "bottom") + 12),
        at((1, 3), (7, 10), (x_of(1, "left") + x_of(2, "right")) / 2, y_of(9, "bottom") + 12),
        at((1, 3), (7, 10), (x_of(1, "left") + x_of(2, "right")) / 2, y_of(7, "top")),
    ], shape="arrow", stroke="onSurfaceMuted"),
    "first": slot((1, 2), (5, 6), "title", "Start", inset=24, align={"x": "center", "y": "center"}),
    "decision": slot((4, 6), (5, 6), "title", "Ready?", align={"x": "center", "y": "center"}),
    "yes": slot((7, 7), (4, 4), "label", "Yes", align={"x": "center", "y": "end"}),
    "second": slot((8, 10), (5, 6), "title", "Ship it", inset=24, align={"x": "center", "y": "center"}),
    "no": slot((6, 6), (8, 8), "label", "No", align={"y": "center"}),
    "third": slot((4, 6), (9, 10), "title", "Fix it", inset=24, align={"x": "center", "y": "center"}),
})

# Three lanes, a row apart, the arrows between them in the rows they leave.
LANES = [(3, 4), (6, 7), (9, 10)]
SWIM = [((3, 5), LANES[0]), ((3, 5), LANES[1]), ((7, 9), LANES[1]), ((7, 9), LANES[2]), ((11, 12), LANES[2])]
layout("Diagrams", "swimlane", "Steps across lanes, one lane for each role: who does what, and where the work changes hands.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-lane": card((1, 12), r) for o, r in zip(ORDINALS, LANES)},
    **{f"{o}-step": box(c, r, fill="surface", inset=12) for o, (c, r) in zip(ORDINALS, SWIM)},
    "first-arrow": arrow((4, 4), (4, 6), (x_of(4, "left") + 67, y_of(4, "bottom") - 12), (x_of(4, "left") + 67, y_of(6, "top") + 12)),
    "second-arrow": arrow((5, 7), (6, 7), (x_of(5, "right") - 12, y_of(6, "bottom") + 12), (x_of(7, "left") + 12, y_of(6, "bottom") + 12)),
    "third-arrow": arrow((8, 8), (7, 9), (x_of(8, "left") + 67, y_of(7, "bottom") - 12), (x_of(8, "left") + 67, y_of(9, "top") + 12)),
    "fourth-arrow": arrow((9, 11), (9, 10), (x_of(9, "right") - 12, y_of(9, "bottom") + 12), (x_of(11, "left") + 12, y_of(9, "bottom") + 12)),
    **{f"{o}-role": slot((1, 2), r, "label", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, LANES, ["Customer", "Sales", "Finance"])},
    **{o: slot(c, r, "body", w, inset=24, align={"y": "center"}) for o, (c, r), w in zip(ORDINALS, SWIM, ["Asks for a quote", "Writes the quote", "Approves it", "Sends the invoice", "Pays"])},
})

# Charts ----------------------------------------------------------------------------------------

layout("Charts", "positioning-map", "Each player placed by two measures, a dot apiece, ours marked: where we stand among them.", {
    "kicker": KICKER,
    "header": header(),
    "main": slot((1, 8), (3, 10), prompt="A scatter of the players", example=chart("scatter")),
    "takeaway": slot((9, 12), (3, 6), "lede", "Where we stand, in a sentence"),
    "note": slot((9, 12), (7, 8), "caption", "Each dot a player, ours marked"),
    "source": slot((9, 12), (9, 10), "caption", "Its source"),
})

# Comparison ------------------------------------------------------------------------------------

# Features down the side, a product to each pair of columns: a check, a full ball, or an empty one,
# each a character every shipped face draws (no shipped face draws a half ball).
FEATURES = [(4, 5), (6, 7), (8, 9), (10, 11)]
PRODUCTS = [(5, 6), (7, 8), (9, 10), (11, 12)]
GRID_MARKS = [["✓", "✓", "✓", "✓"], ["●", "●", "○", "●"], ["●", "○", "○", "●"], ["✓", "○", "○", "✓"]]
layout("Comparison", "feature-grid", "Features down the side and a column for each product or plan, each marked with a check or a ball.", {
    "header": header(),
    "first-card": card((1, 12), FEATURES[0]),
    "third-card": card((1, 12), FEATURES[2]),
    **{f"{o}-product": slot(c, (3, 3), "label", w, align={"x": "center"}) for o, c, w in zip(ORDINALS, PRODUCTS, ["Ours", "Theirs", "Basic", "Pro"])},
    **{o: slot((1, 4), r, "body", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, FEATURES, ["Works offline", "Shares by link", "Keeps a history", "Exports a PDF"])},
    **{f"{o}-{p}": slot(c, r, "title", m, align={"x": "center", "y": "center"}) for o, r, marks in zip(ORDINALS, FEATURES, GRID_MARKS) for p, c, m in zip(ORDINALS, PRODUCTS, marks)},
})

# Frameworks ------------------------------------------------------------------------------------

# Likelihood up the side, impact across, a column and a row to each cell, each cell shaded by the
# two together toward the accent: the theme's second surface, its rule, then its accent.
LIKELY = [4, 5, 6, 7, 8]
IMPACT = [(c, c) for c in range(3, 8)]
SHADES = ["surface-2"] * 4 + ["line"] * 2 + ["accent"] * 3
layout("Frameworks", "risk-matrix", "Each risk placed by how likely it is and how much it would hurt, the cells shaded from calm to grave.", {
    "kicker": KICKER,
    "header": header(),
    **{f"cell-{5 - li}-{ii + 1}": card(c, (r, r), fill=SHADES[(4 - li) + ii]) for li, r in enumerate(LIKELY) for ii, c in enumerate(IMPACT)},
    "likely-title": slot((1, 2), (3, 3), "label", "Likelihood"),
    **{f"likely-{5 - i}": slot((1, 2), (r, r), "caption", w, align={"y": "center"}) for i, r, w in zip(range(5), LIKELY, ["Almost certain", "Likely", "Possible", "Unlikely", "Rare"])},
    **{f"impact-{i + 1}": slot(c, (9, 9), "caption", w, align={"x": "center"}) for i, c, w in zip(range(5), IMPACT, ["Minor", "Moderate", "Major", "Severe", "Critical"])},
    "impact-title": slot((3, 7), (10, 10), "label", "Impact", align={"x": "center"}),
    "first": slot(IMPACT[1], (6, 6), "body", "Hiring", align={"x": "center", "y": "center"}),
    "second": slot(IMPACT[0], (5, 5), "body", "Churn", align={"x": "center", "y": "center"}),
    "third": slot(IMPACT[3], (8, 8), "body", "Outage", align={"x": "center", "y": "center"}),
    "takeaway": slot((9, 12), (3, 6), "lede", "What to watch, and why"),
    "note": slot((9, 12), (7, 8), "caption", "Each risk where it stands today"),
})

# The nine blocks as the canvas sets them: five columns over two, the value in the middle.
CANVAS = {
    "partners": ((1, 2), (3, 8), "Key partners"),
    "activities": ((3, 4), (3, 5), "Key activities"),
    "resources": ((3, 4), (6, 8), "Key resources"),
    "value": ((5, 8), (3, 8), "Value propositions"),
    "relationships": ((9, 10), (3, 5), "Customer relationships"),
    "channels": ((9, 10), (6, 8), "Channels"),
    "segments": ((11, 12), (3, 8), "Customer segments"),
    "costs": ((1, 6), (9, 10), "Cost structure"),
    "revenue": ((7, 12), (9, 10), "Revenue streams"),
}
layout("Frameworks", "business-model-canvas", "The nine blocks of a business model, from its partners to its customers, its costs, and its revenue.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{k}-card": card(c, r) for k, (c, r, _) in CANVAS.items()},
    **{k: slot(c, (r[0], r[0] + 1), "label", w, inset=16) for k, (c, r, w) in CANVAS.items()},
    "value-words": slot((5, 8), (5, 8), "body", "What we give that no one else does", inset=16),
})

# Editorial -------------------------------------------------------------------------------------

layout("Editorial", "case-study", "One customer's story: a picture, the challenge, what was done, and the results.", {
    "kicker": KICKER,
    "header": header("A customer, and what changed"),
    "art": picture((1, 4), (3, 10)),
    "challenge-title": slot((6, 12), (3, 3), "label", "The challenge"),
    "challenge": slot((6, 12), (4, 5), "body", "What stood in their way"),
    "done-title": slot((6, 12), (6, 6), "label", "What we did"),
    "done": slot((6, 12), (7, 8), "body", "What we did about it"),
    "result-figure": slot((6, 8), (9, 10), "figure", "+38%", align={"y": "end"}),
    "result": slot((9, 12), (9, 10), "caption", "What changed, and by how much", align={"y": "end"}),
})

# Pictures --------------------------------------------------------------------------------------

layout("Pictures", "logo-wall", "Twelve customers' or partners' logos, four to a row, each set at one height.", {
    "kicker": KICKER,
    "header": header("Who we work with"),
    **{f"logo-{i + 1}": slot(c, r, prompt="A logo", example=PICTURE, inset=24) for i, (c, r) in enumerate((c, r) for r in [(3, 4), (6, 7), (9, 10)] for c in QUARTERS)},
})

layout("Pictures", "device-mockup", "A screenshot in a laptop's frame, and what it shows beside it.", {
    "kicker": KICKER,
    "header": header(),
    "screen": slot((1, 7), (3, 9), prompt="A screenshot", example=PICTURE, inset=16),
    "frame": slot((1, 7), (3, 9), prompt={"shape": "rect", "stroke": "onSurfaceMuted"}),
    "base": slot((1, 7), (10, 10), prompt={"shape": "polygon", "fill": "line", "points": [[0, 0], [1, 0], [0.97, 0.4], [0.03, 0.4]]}),
    "title": slot((9, 12), (3, 4), "title", "What it shows"),
    "body": slot((9, 12), (5, 8), "body", "What a person does here, in a sentence or two."),
})

# People ----------------------------------------------------------------------------------------

layout("People", "buyer-persona", "A customer as a person: their picture, their name and role, their goals, and what gets in their way.", {
    "kicker": KICKER,
    "art": picture((1, 4), (3, 10), "A picture of them"),
    "name": slot((6, 12), (3, 4), "headline", "A name"),
    "role": slot((6, 12), (5, 5), "label", "What they do"),
    "goals-title": slot((6, 8), (7, 7), "title", "Goals"),
    "goals": slot((6, 8), (8, 10), "body", {"bullet": ["What they want", "What success is"]}),
    "pains-title": slot((10, 12), (7, 7), "title", "Pains"),
    "pains": slot((10, 12), (8, 10), "body", {"bullet": ["What gets in the way", "What they fear"]}),
}, ember={"art": ((1, 4), (3, 11)), "name": ((6, 12), (3, 4)), "role": ((6, 12), (5, 5)), "goals-title": ((6, 8), (7, 7)), "goals": ((6, 8), (8, 11)), "pains-title": ((10, 12), (7, 7)), "pains": ((10, 12), (8, 11))})

# Briefing --------------------------------------------------------------------------------------

QUAD = [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))]
layout("Briefing", "quad-chart", "Four quarters on one slide: a picture and the dates, the objectives, the team, and the milestones.", {
    "kicker": KICKER,
    "header": header(),
    "art": picture((1, 4), (3, 6)),
    **{f"{k}-card": card(c, r) for k, (c, r) in zip(["objectives", "team", "milestones"], QUAD[1:])},
    "dates-title": slot((5, 6), (3, 4), "label", "Dates"),
    "dates": slot((5, 6), (5, 6), "caption", "June to December"),
    **{f"{k}-title": slot(c, (r[0], r[0] + 1), "label", w, inset=24) for k, (c, r), w in zip(["objectives", "team", "milestones"], QUAD[1:], ["Objectives", "Team", "Milestones"])},
    "objectives": slot(QUAD[1][0], (5, 6), "caption", {"bullet": ["The first", "The second"]}, inset=24),
    "team": slot(QUAD[2][0], (9, 10), "caption", "Who leads, and who builds", inset=24),
    "milestones": slot(QUAD[3][0], (9, 10), "caption", {"bullet": ["Design, August", "Launch, December"]}, inset=24),
})

layout("Briefing", "decision-brief", "The bottom line first, then the issue, the options, what we recommend, and the decision we need.", {
    "kicker": KICKER,
    "header": header(),
    "bottom-card": card((1, 12), (3, 4)),
    "bottom-title": slot((1, 3), (3, 4), "label", "Bottom line", inset=24, align={"y": "center"}),
    "bottom": slot((4, 12), (3, 4), "lede", "What we ask, in a sentence", inset=24, align={"y": "center"}),
    **{f"{o}-card": card(c, (6, 10)) for o, c in zip(ORDINALS, QUARTERS)},
    "mark": rule(QUARTERS[2], (6, 6), points=[[0, 0], [1, 0]], stroke="accent"),
    **{f"{o}-title": slot(c, (6, 7), "label", w, inset=24) for o, c, w in zip(ORDINALS, QUARTERS, ["The issue", "The options", "We recommend", "We need"])},
    **{o: slot(c, (8, 10), "body", w, inset=24) for o, c, w in zip(ORDINALS, QUARTERS, ["What is at stake", "What we could do", "What we should do", "Your decision, by when"])},
})

# Classroom -------------------------------------------------------------------------------------

layout("Classroom", "quiz", "A question and four answers, each on its card: add a build to reveal the right one.", {
    "kicker": KICKER,
    "header": header("The question, in a line?"),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, QUAD)},
    **{f"{o}-letter": slot((c[0], c[0]), r, "figure", w, inset=24, align={"y": "center"}) for o, (c, r), w in zip(ORDINALS, QUAD, "ABCD")},
    **{o: slot((c[0] + 1, c[1]), r, "title", "An answer", inset=24, align={"y": "center"}) for o, (c, r) in zip(ORDINALS, QUAD)},
})

# Maps (ADR-0030, PLAN O.1e) ---------------------------------------------------------------------
# A map's place says "A map", and the layout's picture shows the theme's picture art there with a
# pin and a label over it, no provider's map: Insert › Map… fills the place.

MAP = {"map": "backdrop"}


def map_place(col, row, bleed=None):
    return slot(col, row, prompt="A map", bleed=bleed, example=MAP)


layout("Pictures", "map", "A map of a place under a headline, and a line saying what it shows.", {
    "kicker": KICKER,
    "header": header(),
    "art": map_place((1, 12), (3, 9)),
    "note": slot((1, 8), (10, 10), "caption", "What the map shows"),
})

layout("Pictures", "map-words", "A map off the right edge, a headline and its words beside it: where it is, and why it matters.", {
    "art": map_place((7, 12), (1, 12), bleed=True),
    "header": slot((1, 6), (3, 5), "headline", "What this slide says"),
    "body": slot((1, 6), (6, 10), "body", "Where it is, and why it matters, in a sentence or two."),
}, ember={"header": ((1, 6), (2, 4)), "body": ((1, 6), (5, 11))})

layout("Pictures", "map-full", "A map across the whole slide, and a headline over it.", {
    "art": map_place((1, 12), (1, 12), bleed=True),
    "header": slot((2, 9), (2, 3), "headline", "Where it is"),
}, ember={"header": ((2, 9), (2, 3))})

# ---------------------------------------------------------------------------------------------
# The next of ADR-0031's slide types (PLAN S.2).

# Titles ----------------------------------------------------------------------------------------

layout("Titles", "paper-title", "A paper's title page: its title, its authors and where they work, the venue, and their logos.", {
    "title": slot((1, 11), (3, 6), "display", "The paper's title", align={"y": "cap"}),
    "authors": slot((1, 11), (7, 7), "lede", "A. Author, B. Author, C. Author"),
    "affiliations": slot((1, 11), (8, 8), "caption", "¹ A university   ² A laboratory"),
    "venue": slot((1, 7), (10, 11), "label", "A conference, 2026", align={"y": "end"}),
    **{f"{o}-logo": slot(c, (10, 11), prompt="A logo", example=PICTURE, inset=16) for o, c in zip(ORDINALS, [(9, 10), (11, 12)])},
}, ember={"title": ((1, 11), (3, 7)), "authors": ((1, 11), (8, 8)), "affiliations": ((1, 11), (9, 9)), "venue": ((1, 7), (11, 12)), **{f"{o}-logo": (c, (11, 12)) for o, c in zip(ORDINALS, [(9, 10), (11, 12)])}})

# Statements ------------------------------------------------------------------------------------

layout("Statements", "lyrics", "Two lines of a song, centered over a quiet picture, and who wrote it, small.", {
    "art": picture((1, 12), (1, 12), "A quiet picture, edge to edge", bleed=True),
    "words": slot((2, 11), (4, 8), "quote", "The first line of the song\nAnd the line that answers it", align={"x": "center", "y": "center"}),
    "credit": slot((3, 10), (9, 9), "caption", "The song, and who wrote it", align={"x": "center"}),
}, ember={"words": ((2, 11), (4, 9)), "credit": ((3, 10), (10, 10))})

# Numbers ---------------------------------------------------------------------------------------

layout("Numbers", "pictograph", "A share shown as ten figures, as many filled as it counts, and what it means.", {
    "kicker": KICKER,
    "header": header(),
    **{f"figure-{i + 1}": icon((i + 1, i + 1), (3, 7), "user", inset=8, fill="accent" if i < 7 else "line") for i in range(10)},
    "number": slot((1, 3), (8, 9), "figure", "7 in 10", align={"y": "end"}),
    "claim": slot((4, 10), (8, 9), "title", "What the share means", align={"y": "end"}),
    "note": slot((1, 8), (10, 10), "caption", "Its source"),
})

# Diagrams --------------------------------------------------------------------------------------

# A choice at the left, a chance for each way it goes, and the outcomes each chance comes to, with
# their odds: each joined to the next by a line from its right edge to the next one's left.
def mid_y(row):
    return (y_of(row[0], "top") + y_of(row[1], "bottom")) / 2


def join(col, row, start, end):
    """A line in `col` and `row` from `start` to `end`, through the corner a tree's branch takes."""
    corner = ((start[0] + end[0]) / 2, start[1])
    turn = ((start[0] + end[0]) / 2, end[1])
    return rule(col, row, points=[at(col, row, *p) for p in [start, corner, turn, end]], stroke="onSurfaceMuted")


CHANCES = [(4, 5), (8, 9)]
OUTCOMES = [[(3, 3), (6, 6)], [(7, 7), (10, 10)]]
layout("Diagrams", "decision-tree", "A choice, the chances each way it goes, and the outcomes they come to, each with its odds.", {
    "kicker": KICKER,
    "header": header(),
    "choice-card": card((1, 2), (6, 7)),
    **{f"{o}-chance-shape": ring((5, 5), r) for o, r in zip(ORDINALS, CHANCES)},
    **{f"{o}-branch": join((3, 4), (3, 10), (x_of(2, "right"), mid_y((6, 7))), (x_of(5, "left"), mid_y(r))) for o, r in zip(ORDINALS, CHANCES)},
    **{f"{o}-{p}-branch": join((6, 6), (3, 10), (x_of(5, "right"), mid_y(c)), (x_of(7, "left") - 16, mid_y(r))) for o, c, rows in zip(ORDINALS, CHANCES, OUTCOMES) for p, r in zip(["first", "second"], rows)},
    "choice": slot((1, 2), (6, 7), "title", "Build?", inset=24, align={"x": "center", "y": "center"}),
    **{o: slot((3, 4), (r, r), "label", w, align={"x": "center"}) for o, r, w in zip(ORDINALS, [3, 10], ["Build it", "Buy it"])},
    **{f"{o}-{p}-odds": slot((7, 8), r, "label", w, align={"y": "center"}) for o, rows, ws in zip(ORDINALS, OUTCOMES, [["60%", "40%"], ["80%", "20%"]]) for p, r, w in zip(["first", "second"], rows, ws)},
    **{f"{o}-{p}": slot((9, 12), r, "body", w, align={"y": "center"}) for o, rows, ws in zip(ORDINALS, OUTCOMES, [["Wins the market", "Late to it"], ["Ready in time", "Falls short"]]) for p, r, w in zip(["first", "second"], rows, ws)},
})

# The effect at the head, a spine to it, and a bone above and below for each kind of cause.
SPINE = 7
BONES = [((1, 3), "top"), ((5, 7), "top"), ((1, 3), "bottom"), ((5, 7), "bottom")]
layout("Diagrams", "fishbone", "An effect at the head, and its causes along the bones, a kind of cause to each.", {
    "kicker": KICKER,
    "header": header(),
    "spine": rule((1, 10), (SPINE, SPINE), shape="arrow", stroke="onSurfaceMuted"),
    **{f"{o}-bone": rule((c[0], c[1] + 1), (5, 7) if side == "top" else (7, 9), points=[
        at((c[0], c[1] + 1), (5, 7) if side == "top" else (7, 9), x_of(c[1] + 1, "right"), mid_y((SPINE, SPINE))),
        at((c[0], c[1] + 1), (5, 7) if side == "top" else (7, 9), x_of(c[0], "left") + 67, y_of(5, "top") if side == "top" else y_of(9, "bottom")),
    ], stroke="onSurfaceMuted") for o, (c, side) in zip(ORDINALS, BONES)},
    "head-card": card((11, 12), (6, 8)),
    "head": slot((11, 12), (6, 8), "title", "The effect", inset=24, align={"y": "center"}),
    **{f"{o}-title": slot(c, (3, 3) if side == "top" else (10, 10), "label", w) for o, (c, side), w in zip(ORDINALS, BONES, ["People", "Process", "Tools", "Materials"])},
    **{o: slot(c, (4, 4) if side == "top" else (11, 11), "caption", "A cause") for o, (c, side) in zip(ORDINALS, BONES)},
})

# Three tiers of boxes, each joined to the tier under it.
TIERS = [(3, 4), (6, 7), (9, 10)]
BOXES = [(4, 5), (7, 8), (10, 11)]
layout("Diagrams", "architecture", "Systems as boxes in tiers, each joined by arrows to what it calls.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{t}-{o}-card": card(c, TIERS[i]) for i, t in enumerate(["client", "service"]) for o, c in zip(ORDINALS, BOXES)},
    "data-card": card((4, 11), TIERS[2]),
    **{f"{o}-call": rule(c, (5, 5), points=[[0.5, 0], [0.5, 1]], shape="arrow", stroke="onSurfaceMuted") for o, c in zip(ORDINALS, BOXES)},
    **{f"{o}-query": rule(c, (8, 8), points=[[0.5, 0], [0.5, 1]], shape="arrow", stroke="onSurfaceMuted") for o, c in zip(ORDINALS, BOXES)},
    **{f"{t}-tier": slot((1, 3), r, "label", w, align={"y": "center"}) for t, r, w in zip(["client", "service", "data"], TIERS, ["Clients", "Services", "Data"])},
    **{f"{t}-{o}": slot(c, TIERS[i], "body", w, inset=16, align={"x": "center", "y": "center"}) for i, (t, ws) in enumerate(zip(["client", "service"], [["Web", "Mac", "iPad"], ["Accounts", "Billing", "Search"]])) for o, c, w in zip(ORDINALS, BOXES, ws)},
    "data": slot((4, 11), TIERS[2], "body", "The database", inset=16, align={"x": "center", "y": "center"}),
})

# The question at the left, two parts of it, and two questions each part comes to, left to right.
PARTS = [(3, 5), (8, 10)]
SUBS = [[(3, 3), (5, 5)], [(8, 8), (10, 10)]]
layout("Diagrams", "issue-tree", "A question split into the questions it comes to, left to right, two levels deep.", {
    "kicker": KICKER,
    "header": header(),
    "root-card": card((1, 3), (5, 8)),
    **{f"{o}-card": card((5, 7), r) for o, r in zip(ORDINALS, PARTS)},
    **{f"{o}-branch": join((4, 4), (3, 10), (x_of(3, "right"), mid_y((5, 8))), (x_of(5, "left"), mid_y(r))) for o, r in zip(ORDINALS, PARTS)},
    **{f"{o}-{p}-branch": join((8, 8), (3, 10), (x_of(7, "right"), mid_y(r)), (x_of(9, "left") - 16, mid_y(s))) for o, r, subs in zip(ORDINALS, PARTS, SUBS) for p, s in zip(["first", "second"], subs)},
    "root": slot((1, 3), (5, 8), "title", "The question", inset=24, align={"y": "center"}),
    **{o: slot((5, 7), r, "body", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, PARTS, ["A part of it", "Another part"])},
    **{f"{o}-{p}": slot((9, 12), s, "caption", "A question it comes to", align={"y": "center"}) for o, subs in zip(ORDINALS, SUBS) for p, s in zip(["first", "second"], subs)},
})

# Frameworks ------------------------------------------------------------------------------------

# Five levels, each a step higher than the one before, and where we stand marked over it.
LEVELS = [((2 * k + 1, 2 * k + 2), (9 - k, 10)) for k in range(5)]
layout("Frameworks", "maturity-model", "Levels of maturity as steps rising one after another, and where we stand among them.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-step": card(c, r) for o, (c, r) in zip(ORDINALS, LEVELS)},
    "here-arrow": rule(LEVELS[2][0], (6, 6), points=[[0.5, 0], [0.5, 1]], shape="arrow", stroke="accent"),
    "here": slot(LEVELS[2][0], (5, 5), "label", "We are here", align={"x": "center", "y": "end"}),
    "takeaway": slot((1, 4), (3, 5), "lede", "Where we stand, and the next step"),
    **{o: slot(c, (r[0], r[0] + 1) if r[0] < 9 else r, "body", w, inset=16) for o, (c, r), w in zip(ORDINALS, LEVELS, ["Ad hoc", "Repeatable", "Defined", "Managed", "Leading"])},
})

# Briefing --------------------------------------------------------------------------------------

layout("Briefing", "incident-review", "What happened, what it hurt, why, and what we'll do so it doesn't happen again.", {
    "kicker": KICKER,
    "header": header("What happened, in a line"),
    **{f"{o}-card": card(c, r) for o, (c, r) in zip(ORDINALS, QUAD)},
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "label", w, inset=24) for o, (c, r), w in zip(ORDINALS, QUAD, ["What happened", "The impact", "The root cause", "What we'll do"])},
    **{o: slot(c, (r[0] + 2, r[1]), "caption", w, inset=24) for o, (c, r), w in zip(ORDINALS, QUAD, ["When it began, and when it ended", "Who it reached, and how badly", "Why it happened", {"bullet": ["The first fix", "The second"]}])},
})

# Options down the side, criteria across, each scored, and the total; the one chosen outlined.
OPTION_ROWS = [(4, 5), (6, 7), (8, 9)]
CRITERIA = [(5, 6), (7, 8), (9, 10)]
layout("Briefing", "options-compared", "Courses of action scored against the criteria, each with its total, the one chosen outlined.", {
    "kicker": KICKER,
    "header": header(),
    "first-card": card((1, 12), OPTION_ROWS[0]),
    "third-card": card((1, 12), OPTION_ROWS[2]),
    "chosen": ring((1, 12), OPTION_ROWS[1], shape="rect"),
    "option-label": slot((1, 4), (3, 3), "label", "Option"),
    **{f"{o}-criterion": slot(c, (3, 3), "label", w, align={"x": "center"}) for o, c, w in zip(ORDINALS, CRITERIA, ["Cost", "Speed", "Risk"])},
    "total-label": slot((11, 12), (3, 3), "label", "Total", align={"x": "center"}),
    **{o: slot((1, 4), r, "body", w, inset=24, align={"y": "center"}) for o, r, w in zip(ORDINALS, OPTION_ROWS, ["Wait", "Build", "Buy"])},
    **{f"{o}-{p}": slot(c, r, "body", w, align={"x": "center", "y": "center"}) for o, r, ws in zip(ORDINALS, OPTION_ROWS, [["3", "1", "2"], ["2", "3", "3"], ["1", "3", "2"]]) for p, c, w in zip(ORDINALS, CRITERIA, ws)},
    **{f"{o}-total": slot((11, 12), r, "figure", w, align={"x": "center", "y": "center"}) for o, r, w in zip(ORDINALS, OPTION_ROWS, ["6", "8", "6"])},
})

# Time ------------------------------------------------------------------------------------------

layout("Time", "month", "A month as a calendar, a week to each row, with what falls on each day.", {
    "kicker": KICKER,
    "header": header("The month, and what falls in it"),
    "main": slot((1, 12), (3, 10), prompt="A month's calendar", example={"table": 7}),
})

layout("Time", "event", "A date set large, what is happening and where, and a picture beside them: a save-the-date or an announcement.", {
    "art": picture((8, 12), (1, 12), "A picture, off the edge", bleed=True),
    "tag": slot((1, 6), (1, 1), "label", "Save the date"),
    "date": slot((1, 6), (2, 6), "hero", "Nov 12", align={"y": "end"}, fit="shrink"),
    "what": slot((1, 6), (8, 9), "title", "What's happening"),
    "where": slot((1, 6), (10, 10), "body", "7 pm, and where"),
}, ember={"tag": ((1, 6), 1), "date": ((1, 6), (2, 7)), "what": ((1, 6), (9, 10)), "where": ((1, 6), (11, 12))})

# Pictures --------------------------------------------------------------------------------------

layout("Pictures", "property", "A home or a place for sale: a picture off the left edge, its price, its rooms and size, and a second picture.", {
    "art": picture((1, 8), (1, 12), "A picture of the place", bleed=True),
    "price": slot((9, 12), (1, 3), "hero", "$1.2M", align={"y": "end"}, fit="shrink"),
    "address": slot((9, 12), (5, 5), "title", "The address"),
    **{k: slot((9, 12), (r, r), "body", w) for k, r, w in zip(["beds", "baths", "area"], [6, 7, 8], ["3 beds", "2 baths", "1,800 sq ft"])},
    "second-art": picture((9, 12), (10, 12), "Another picture"),
}, ember={"price": ((9, 12), (1, 3)), "address": ((9, 12), (5, 5)), "beds": ((9, 12), 6), "baths": ((9, 12), 7), "area": ((9, 12), 8), "second-art": ((9, 12), (10, 12))})

# Editorial -------------------------------------------------------------------------------------

layout("Editorial", "certificate", "A certificate inside its border: who receives it, for what, the date, and who signs it.", {
    "border": ring((1, 12), (1, 12), shape="rect"),
    "kind": slot((2, 11), (2, 2), "label", "Certificate of achievement", align={"x": "center"}),
    "presented": slot((2, 11), (3, 3), "caption", "Presented to", align={"x": "center"}),
    "name": slot((2, 11), (4, 6), "display", "A name", align={"x": "center", "y": "cap"}),
    "for": slot((3, 10), (7, 8), "lede", "For what they achieved", align={"x": "center"}),
    **{f"{o}-line": rule(c, (10, 10), points=[[0, 1], [1, 1]]) for o, c in zip(ORDINALS, [(2, 5), (8, 11)])},
    **{f"{o}-sign": slot(c, (11, 11), "caption", w, align={"x": "center"}) for o, c, w in zip(ORDINALS, [(2, 5), (8, 11)], ["Who signs it", "The date"])},
}, ember={"border": ((1, 12), (1, 12)), "kind": ((2, 11), 2), "presented": ((2, 11), 3), "name": ((2, 11), (4, 7)), "for": ((3, 10), (8, 9)), "first-line": ((2, 5), 10), "second-line": ((8, 11), 10), "first-sign": ((2, 5), 11), "second-sign": ((8, 11), 11)})

layout("Editorial", "references", "The works cited, numbered, in small type.", {
    "kicker": KICKER,
    "header": header("References"),
    "body": slot((1, 10), (3, 10), "caption", {"number": ["An author. A title. A journal, 2026.", "An author. A title. A publisher, 2025.", "An author. A title. A site, 2024."]}),
})

layout("Editorial", "patient-case", "A patient's case: how they came in, their history, what was found, and an image of it.", {
    "kicker": KICKER,
    "header": header("A case, in a line"),
    "art": picture((8, 12), (3, 10), "An image of what was found"),
    **{f"{k}-title": slot((1, 6), (r, r), "label", w) for k, r, w in zip(["presentation", "history", "findings"], [3, 6, 9], ["Presentation", "History", "Findings"])},
    **{k: slot((1, 6), (r + 1, r + 2), "body", w) for k, r, w in zip(["presentation", "history", "findings"], [3, 6, 9], ["Who came in, and why", "What came before", "What the tests showed"])},
})

# Classroom -------------------------------------------------------------------------------------

POINTS = [(4, 5), (6, 7), (8, 9)]
layout("Classroom", "game-board", "A board of points under four categories, each card to link to its question.", {
    "kicker": KICKER,
    "header": header("The game"),
    **{f"{o}-{p}-card": card(c, r) for o, c in zip(ORDINALS, QUARTERS) for p, r in zip(ORDINALS, POINTS)},
    **{f"{o}-category": slot(c, (3, 3), "label", w, align={"x": "center"}) for o, c, w in zip(ORDINALS, QUARTERS, ["A category", "Another", "A third", "A fourth"])},
    **{f"{o}-{p}": slot(c, r, "figure", w, align={"x": "center", "y": "center"}) for o, c in zip(ORDINALS, QUARTERS) for p, r, w in zip(ORDINALS, POINTS, ["100", "200", "300"])},
})

# Closing ---------------------------------------------------------------------------------------

layout("Closing", "acknowledgments", "Who made it possible: thanks, the people by name, and the funders' and partners' logos.", {
    "kicker": KICKER,
    "header": header("With thanks"),
    "thanks": slot((1, 6), (3, 5), "lede", "To the people and funders who made this possible"),
    "names": slot((1, 6), (6, 10), "body", "Our collaborators, by name"),
    **{f"logo-{i + 1}": slot(c, r, prompt="A logo", example=PICTURE, inset=24) for i, (c, r) in enumerate((c, r) for r in [(3, 4), (6, 7), (9, 10)] for c in [(7, 9), (10, 12)])},
})

layout("Closing", "join-or-scan", "A code to scan and the link to type: to join, to give, or to download.", {
    "kicker": KICKER,
    "code": slot((2, 4), (3, 8), prompt="A code to scan", example=PICTURE),
    "ask": slot((6, 12), (3, 5), "display", "Join us", align={"y": "cap"}),
    "link": slot((6, 12), (6, 7), "title", "example.org/join"),
    "how": slot((6, 12), (8, 8), "caption", "Scan the code, or type the link"),
})

# ---------------------------------------------------------------------------------------------
# The first five of ADR-0031's last slide types (PLAN S.3): those that need nothing new.

# Diagrams --------------------------------------------------------------------------------------

# Six tables in two rows of three, a circle in the middle of each third, each its number in the
# middle and its guests down either side of it.
SEATS = [(c, r) for r in [(3, 6), (7, 10)] for c in [(2, 3), (6, 7), (10, 11)]]
GUESTS = [("Ana\nBen", "Cy\nDee"), ("Eve\nFin", "Gus\nHal"), ("Ida\nJo", "Kai\nLu"), ("Max\nNia", "Oli\nPam"), ("Quin\nRo", "Sam\nTess"), ("Uma\nVic", "Wes\nZoe")]
layout("Diagrams", "seating-chart", "Tables as circles, each its number in the middle and its guests around it: who sits where.", {
    "kicker": KICKER,
    "header": header("Where everyone sits"),
    **{f"{o}-table": slot(c, r, prompt={"shape": "ellipse", "fill": "surface-2"}) for o, (c, r) in zip(ORDINALS, SEATS)},
    **{o: slot(c, r, "figure", str(i + 1), align={"x": "center", "y": "center"}) for i, (o, (c, r)) in enumerate(zip(ORDINALS, SEATS))},
    **{f"{o}-left": slot((c[0] - 1, c[0] - 1), r, "caption", g[0], align={"x": "center", "y": "center"}) for o, (c, r), g in zip(ORDINALS, SEATS, GUESTS)},
    **{f"{o}-right": slot((c[1] + 1, c[1] + 1), r, "caption", g[1], align={"x": "center", "y": "center"}) for o, (c, r), g in zip(ORDINALS, SEATS, GUESTS)},
}, ember={
    # Ember's rows a row lower, each table as tall as on Dusk's grid, not stretched to the foot.
    f"{o}{k}": ((c[0] - 1, c[0] - 1) if k == "-left" else (c[1] + 1, c[1] + 1) if k == "-right" else c, (r[0] + 1, r[1] + 1))
    for o, (c, r) in zip(ORDINALS, SEATS) for k in ["-table", "", "-left", "-right"]
})

# Pictures --------------------------------------------------------------------------------------

# The page at the left, its passage ringed, and the passage enlarged beside it.
PASSAGE = (6, 7)
layout("Pictures", "exhibit-callout", "A page of a document with its key passage ringed, and the passage enlarged beside it.", {
    "kicker": KICKER,
    "header": header("What the document says"),
    "page": slot((1, 5), (3, 10), prompt="A page of the document", example=PICTURE),
    "ring": ring((2, 5), PASSAGE, shape="rect"),
    "pointer": arrow((6, 6), PASSAGE, (x_of(6, "left"), mid_y(PASSAGE)), (x_of(6, "right"), mid_y(PASSAGE)), stroke="accent"),
    "passage-card": card((7, 12), (3, 9)),
    "passage": slot((7, 12), (3, 8), "quote", "The passage, enlarged", inset=48, align={"y": "center"}),
    "source": slot((7, 12), (10, 10), "caption", "The exhibit, and its page"),
})

# Briefing --------------------------------------------------------------------------------------

# Phases across, as chevrons, each pointing to the next; the lines of effort down the side; and a
# card in each cell for what that line does in that phase.
PHASES = [(4, 6), (7, 9), (10, 12)]
EFFORTS = [(4, 5), (6, 7), (8, 9)]
layout("Briefing", "concept-of-operations", "Phases across and lines of effort down, a card in each cell for what each line does in each phase.", {
    "kicker": KICKER,
    "header": header(),
    **{f"{o}-phase-shape": slot(c, (3, 3), prompt={"shape": "polygon", "fill": "surface-2", "points": CHEVRON_FIRST if i == 0 else CHEVRON}) for i, (o, c) in enumerate(zip(ORDINALS, PHASES))},
    **{f"{p}-{o}-card": box(c, r, inset=8) for p, r in zip(ORDINALS, EFFORTS) for o, c in zip(ORDINALS, PHASES)},
    **{f"{o}-phase": slot(c, (3, 3), "label", w, align={"x": "center", "y": "center"}) for o, c, w in zip(ORDINALS, PHASES, ["Prepare", "Act", "Hand over"])},
    **{f"{p}-effort": slot((1, 3), r, "body", w, align={"y": "center"}) for p, r, w in zip(ORDINALS, EFFORTS, ["People", "Systems", "Partners"])},
    **{f"{p}-{o}": slot(c, r, "body", w, inset=24, align={"y": "center"}) for p, r, ws in zip(ORDINALS, EFFORTS, [["Hire", "Train", "Rotate"], ["Build", "Run", "Retire"], ["Brief", "Join", "Lead"]]) for o, c, w in zip(ORDINALS, PHASES, ws)},
})

# Editorial -------------------------------------------------------------------------------------

# The logo and how to use it at the left; the palette's swatches, each named, and the type at the
# right.
SWATCHES = [(6, 7), (8, 9), (10, 11)]
layout("Editorial", "brand-board", "A brand on one slide: its logo and how to use it, its colors, and its type.", {
    "kicker": KICKER,
    "header": header("The brand, at a glance"),
    "logo": slot((1, 4), (3, 7), prompt="The logo", example=PICTURE, inset=24),
    "logo-frame": slot((1, 4), (3, 7), prompt={"shape": "rect", "stroke": "line"}),
    "logo-use": slot((1, 4), (8, 10), "caption", "Clear space all round, the height of the mark. Never stretched or recolored."),
    **{f"{o}-swatch": card(c, (3, 5), fill=f) for o, c, f in zip(ORDINALS, SWATCHES, ["accent", "accent-2", "surface-2"])},
    **{o: slot(c, (6, 6), "label", w) for o, c, w in zip(ORDINALS, SWATCHES, ["Primary", "Second", "Ground"])},
    "type-sample": slot((6, 7), (8, 10), "display", "Aa", align={"y": "cap"}),
    "type-title": slot((8, 12), (8, 9), "headline", "Our headlines"),
    "type": slot((8, 12), (10, 10), "body", "And the words under them"),
})

layout("Editorial", "fine-print", "A safe-harbor statement or a disclaimer, in small type: what the audience is told before or after the talk.", {
    "kicker": KICKER,
    "header": header("Forward-looking statements"),
    "body": slot((1, 10), (3, 10), "caption", "This presentation contains forward-looking statements. They involve risks and uncertainties, and actual results may differ materially. We assume no obligation to update them."),
})

# ---------------------------------------------------------------------------------------------
# The 47 the themes had: the section each is offered in, and what its picture shows in each slot
# that waits.

OLD = {
    "Titles": ["title", "section", "photo-title", "title-only"],
    "Statements": ["statement", "poster", "quote"],
    "Lists": ["bullets", "numbered", "agenda", "three-rows"],
    "Comparison": ["two-columns", "comparison", "pros-cons", "three-columns", "cards"],
    "Frameworks": ["quadrants", "funnel", "pyramid"],
    "Process": ["steps"],
    "Time": ["roadmap", "timeline"],
    "Numbers": ["stat", "big-number", "two-numbers", "numbers", "number-grid"],
    "Charts": ["figure", "narrow-figure", "split", "figure-numbers"],
    "Pictures": ["full", "art-left", "art-right", "photo", "photo-caption", "photo-left", "photo-right", "two-up", "three-up", "four-up", "mosaic"],
    "People": ["team", "testimonials", "quote-portrait"],
    "Editorial": ["definition"],
    "Closing": ["closing"],
}

# What changes in the layouts the themes had, slot by slot: each key set here in place of the
# slot's own. A number is set as Big Number sets it, and shrinks to fit its slot where it is too
# wide for it (Jay's call, 2026-10-11).
OLD_SLOTS = {
    # A section's title stands on what supports it, as a cover's does, however many lines it takes
    # (Jay's review of the climate deck, 2026-10-12).
    "section": {"title": {"align": {"x": "start", "y": "end"}}},
    "stat": {"number": {"role": "hero", "fit": "shrink"}},
    "big-number": {"number": {"fit": "shrink"}},
    "two-numbers": {"first-title": {"fit": "shrink"}, "second-title": {"fit": "shrink"}},
    # A list's points are read from the back of the room: set in the lede, as Statements' credo
    # sets its own, not in the body's size, which leaves three of them in the top of the slot.
    "bullets": {"body": {"role": "lede"}},
    # The words stand on who said them, or on what supports them, a gutter apart, whatever lines
    # they take: the two read as one piece.
    "quote": {"quote": {"align": {"x": "start", "y": "end"}}},
    "statement": {"statement": {"align": {"x": "start", "y": "end"}}},
    "numbered": {"body": {"role": "lede"}},
}

# And in one theme alone. Where Stat's claim sits under its number, as in Dusk and Daybreak, the
# number ends a row higher, as Big Number's does: a number in `hero` reaches below its baseline,
# and a row left empty keeps it off the claim. A timeline's dates stand on its rule across the
# middle of what the slide holds, from the row `top`, and what happened hangs under it to `last`.
# A title slide's byline stands at its foot under the title and the line on what it is about, as
# theirs do, not under the picture, where it would read as the picture's caption (Ember's runs
# along the top).


def timeline_rows(top, last=10):
    dates = {f"{o}-title": {"row": [top, top + 2], "align": {"y": "end"}} for o in ORDINALS[:4]}
    happened = {o: {"row": [top + 4, last]} for o in ORDINALS[:4]}
    return {**dates, "rule": {"row": [top + 3, top + 3]}, **happened}


TITLE_KICKER = {"kicker": {"col": [1, 8], "row": [11, 12], "align": {"y": "end"}}}

# By geometry family.
OLD_SLOTS_IN = {
    "dusk": {"stat": {"number": {"row": [3, 7]}}, "timeline": timeline_rows(3), "title": TITLE_KICKER},
    "ember": {"timeline": timeline_rows(4, last=12)},
}

OLD_EXAMPLES = {
    "title": {"art": PICTURE},
    "poster": {"art": PICTURE},
    "full": {"main": PICTURE},
    "figure": {"main": chart("bar")},
    "narrow-figure": {"main": chart("bar")},
    "split": {"main": chart("line")},
    "art-left": {"art": PICTURE},
    "art-right": {"art": PICTURE},
    "photo-title": {"art": PICTURE},
    "quote-portrait": {"art": PICTURE},
    "photo": {"art": PICTURE},
    "photo-caption": {"art": PICTURE},
    "photo-left": {"art": PICTURE},
    "photo-right": {"art": PICTURE},
    "two-up": {"first-art": PICTURE, "second-art": PICTURE},
    "three-up": {"first-art": PICTURE, "second-art": PICTURE, "third-art": PICTURE},
    "four-up": {"first-art": PICTURE, "second-art": PICTURE, "third-art": PICTURE, "fourth-art": PICTURE},
    "mosaic": {"first-art": PICTURE, "second-art": PICTURE, "third-art": PICTURE},
    "team": {"first-art": PICTURE, "second-art": PICTURE, "third-art": PICTURE},
    "figure-numbers": {"main": chart("line")},
}

# Each section's layouts, in the order Add Slide offers them: those the themes had first.
ORDER = {s: OLD.get(s, []) + [n for n, l in NEW.items() if l["section"] == s] for s in SECTIONS}

# What a person calls each layout in an editor (theme format 0.12): the name a presentation app
# gives such a slide, in its capitals.
NAMES = {
    # Titles
    "title": "Title", "section": "Section", "photo-title": "Title over Photo", "title-only": "Title Only",
    "title-center": "Centered Title", "title-split": "Title beside Photo", "section-photo": "Section and Photo",
    "section-number": "Section Number",
    # Statements
    "statement": "Statement", "poster": "Poster", "quote": "Quote", "thesis": "Thesis", "big-question": "Big Question",
    "manifesto": "Manifesto", "key-point": "Key Point", "haiku": "Haiku",
    # Lists
    "bullets": "Bullets", "numbered": "Numbered List", "agenda": "Agenda", "three-rows": "Three Rows",
    "bullets-photo": "Bullets and Photo", "checklist": "Checklist", "agenda-photo": "Agenda and Photo",
    "four-points": "Four Points", "numbered-cards": "Numbered Cards", "icon-columns": "Icon Columns",
    "icon-cards": "Icon Cards", "icon-rows": "Icon Rows", "icon-grid": "Icon Grid",
    # Comparison
    "two-columns": "Two Columns", "comparison": "Comparison", "pros-cons": "Pros and Cons",
    "three-columns": "Three Columns", "cards": "Cards", "before-after": "Before and After",
    "head-to-head": "Head to Head", "scenarios": "Scenarios", "myth-fact": "Myth and Fact", "pricing": "Pricing",
    # Frameworks
    "quadrants": "Quadrants", "funnel": "Funnel", "pyramid": "Pyramid", "matrix": "Two-by-Two Matrix",
    "swot": "SWOT", "okrs": "OKRs", "raci": "RACI", "three-horizons": "Three Horizons", "venn": "Venn Diagram",
    "cycle": "Cycle", "nine-box": "Nine-Box Grid", "stack": "Stack", "hub-spoke": "Hub and Spoke",
    # Process
    "steps": "Steps", "process": "Process", "how-it-works": "How It Works", "pipeline": "Pipeline",
    "journey": "Journey", "recipe": "Recipe",
    # Time
    "roadmap": "Roadmap", "timeline": "Timeline", "milestones": "Milestones", "quarters": "Quarters",
    "gantt": "Gantt Chart", "year": "Year at a Glance", "timeline-photos": "Timeline with Photos",
    # Numbers
    "stat": "Stat", "big-number": "Big Number", "two-numbers": "Two Numbers", "numbers": "Three Numbers",
    "number-grid": "Six Numbers", "two-figures": "Two Figures", "four-numbers": "Four Numbers", "scorecard": "Scorecard", "ranking": "Ranking", "progress": "Progress",
    "number-photo": "Number and Photo", "metric-trend": "Number and Trend",
    # Charts
    "figure": "Chart", "narrow-figure": "Narrow Chart", "split": "Chart and Words",
    "figure-numbers": "Chart and Numbers", "bar-chart": "Bar Chart", "line-chart": "Line Chart",
    "donut-chart": "Donut Chart", "table": "Table", "chart-bullets": "Chart and Bullets", "two-charts": "Two Charts",
    # Pictures
    "full": "Full Bleed", "art-left": "Art Left", "art-right": "Art Right", "photo": "Photo",
    "photo-caption": "Photo and Caption", "photo-left": "Photo Left", "photo-right": "Photo Right",
    "two-up": "Two Up", "three-up": "Three Up", "four-up": "Four Up", "mosaic": "Mosaic", "six-up": "Six Up",
    "panorama": "Panorama", "photo-quote": "Quote over Photo", "four-in-a-row": "Four in a Row",
    "callout": "Callouts", "photo-grid-text": "Photo Grid and Words",
    # People
    "team": "Team", "testimonials": "Testimonials", "quote-portrait": "Quote and Portrait", "bio": "Bio",
    "team-four": "Team of Four", "team-six": "Team of Six", "team-names": "Team by Name", "org-chart": "Org Chart", "speaker": "Speaker",
    "two-quotes": "Two Quotes",
    # Editorial
    "definition": "Definition", "letter": "Letter", "memo": "Memo", "receipt": "Receipt",
    "chyron": "Lower Third", "headline": "Front Page", "faq": "FAQ", "glossary": "Glossary",
    "magazine": "Magazine Spread", "case-study": "Case Study",
    # Closing
    "closing": "Closing", "next-steps": "Next Steps", "the-ask": "The Ask", "qa": "Q&A", "contact": "Contact",
    "takeaways": "Takeaways", "call-to-action": "Call to Action",
    # Diagrams, Briefing, and Classroom (PLAN S.1)
    "flowchart": "Flowchart", "swimlane": "Swimlane", "quad-chart": "Quad Chart", "decision-brief": "Decision Brief",
    "quiz": "Quiz",
    # The rest of S.1's, by the sections above
    # S.2's
    "paper-title": "Paper Title", "lyrics": "Lyrics", "pictograph": "Pictograph", "decision-tree": "Decision Tree",
    "fishbone": "Fishbone", "architecture": "Architecture", "issue-tree": "Issue Tree",
    "maturity-model": "Maturity Model", "incident-review": "Incident Review", "options-compared": "Options Compared",
    "month": "Month", "event": "Event", "property": "Property", "certificate": "Certificate",
    "references": "References", "patient-case": "Patient Case", "game-board": "Game Board",
    "acknowledgments": "Acknowledgments", "join-or-scan": "Join or Scan",
    "kpi-dashboard": "KPI Dashboard", "status-report": "Status Report", "market-size": "Market Size",
    "positioning-map": "Positioning Map", "feature-grid": "Feature Grid", "risk-matrix": "Risk Matrix",
    "business-model-canvas": "Business Model Canvas", "logo-wall": "Logo Wall", "device-mockup": "Device Mockup",
    "buyer-persona": "Buyer Persona",
    # S.3's first five
    "seating-chart": "Seating Chart", "exhibit-callout": "Exhibit Callout",
    "concept-of-operations": "Concept of Operations", "brand-board": "Brand Board", "fine-print": "Fine Print",
    # Maps (PLAN O.1e)
    "map": "Map", "map-words": "Map and Words", "map-full": "Full-Bleed Map",
}

# The descriptions of the layouts the themes had, where one named what a person never reads in an
# editor's window, a slot or a lint code (PLAN 3.18): by geometry family, `*` for each.
DESCRIPTIONS = {
    "title": {"*": "The opening: the talk's title, a line on what it's about, and a picture beside them."},
    "figure": {
        "dusk": "A headline, the figure that shows it, and a note on it, each in its own rows: the data on the plain surface, with no picture behind it.",
    },
    "narrow-figure": {"*": "A headline, a chart of two or three bars, and a note on it beside the chart."},
    "split": {"*": "Words, and the chart or table beside them: the data on the plain surface, with no picture behind it."},
    "full": {
        "dusk": "A headline over one thing that fills the grid, a picture, a chart, or a table, and a note on it.",
    },
}


# ---------------------------------------------------------------------------------------------
# Ember's slots for a layout added here: its kicker along the top, its headline in rows 2–3, and
# what the slide holds a row lower than on Dusk's grid, so bands that repeat keep one height. A
# slot of four rows or more that reaches the foot of what the slide holds stretches to Ember's last
# row, as its own layouts' figures and cards do; one that stands at the foot, under the rest, goes
# to row 12. A layout says where a slot goes where this would not do.


def ember_slots(name, slots, told):
    content = [s for k, s in slots.items() if k != "kicker" and s["row"] != [1, 12] and s["row"][0] >= 3]
    bottom = max((s["row"][1] for s in content), default=10)
    foot = bottom > 10 and all(s["row"][0] >= 11 or s["row"][1] <= 10 for s in content)
    out = {}
    for key, s in slots.items():
        e = dict(s)
        if key in told:
            col, row = told[key]
            e["col"] = list(col)
            e["row"] = row if isinstance(row, int) else list(row)
        elif key == "kicker":
            e["col"], e["row"] = [1, 12], 1
            e.pop("align", None)
        elif s["row"] == [1, 2]:
            e["row"] = [2, 3]
        elif s["row"][0] >= 11 and foot:
            e["row"] = [12, 12]
        elif s["row"][0] >= 3 and s["row"] != [1, 12]:
            a, b = s["row"]
            end = min(b + 1, 12)
            if not foot and b == bottom == 10 and b - a + 1 >= 4:
                end = 12
            e["row"] = [a + 1, end]
        out[key] = e
    return out


# ---------------------------------------------------------------------------------------------
# Writing.


def words(slots):
    n = 0
    for s in slots.values():
        p = s.get("prompt")
        if not s.get("role") or p is None:
            continue
        if isinstance(p, str):
            n += len(p.split())
        elif isinstance(p, dict) and ("bullet" in p or "number" in p):
            n += sum(len(i.split()) for i in p.get("bullet", p.get("number", [])))
    return n


def cells(s):
    col, row = s["col"], s["row"]
    col = col if isinstance(col, list) else [col, col]
    row = row if isinstance(row, list) else [row, row]
    return col, row


def overlaps(a, b):
    (ca, ra), (cb, rb) = cells(a), cells(b)
    return ca[0] <= cb[1] and cb[0] <= ca[1] and ra[0] <= rb[1] and rb[0] <= ra[1]


def checked(theme, name, slots, new):
    """No more than 40 words in a slide the layout starts; and in a layout added here, no two slots
    of words overlap and each stands on the grid. The themes' own may lay a slot over another's
    room where its words leave it, as Ember's `stat` does: lint judges the words, not the slots."""
    n = words(slots)
    assert n <= MAX_WORDS, f"{theme}'s {name} starts a slide of {n} words"
    if not new:
        return
    texts = [(k, s) for k, s in slots.items() if s.get("role")]
    for i, (k, a) in enumerate(texts):
        for j, b in texts[i + 1:]:
            assert not overlaps(a, b), f"{theme}'s {name}: `{k}` and `{j}` overlap"
        col, row = cells(a)
        assert 1 <= col[0] <= col[1] <= 12 and 1 <= row[0] <= row[1] <= 12, f"{theme}'s {name}: `{k}` is off the grid"


def library(theme, old, geometry):
    """The theme's layouts, in the library's order: those it had, put in their sections and given
    their examples, and those added here, Ember's made from Dusk's, each as its geometry family
    lays it out."""
    out = {}
    for section in SECTIONS:
        for name in ORDER[section]:
            if name in NEW:
                spec = NEW[name]
                slots = spec["slots"]
                if geometry == "ember":
                    slots = ember_slots(name, slots, spec["ember"])
                layout = {"name": NAMES[name], "description": spec["description"], "group": section, "slots": slots}
            else:
                assert name in old, f"{theme} has no layout `{name}`"
                had = json.loads(json.dumps(old[name]))
                told = DESCRIPTIONS.get(name, {})
                described = told.get(geometry, told.get("*", had.get("description")))
                layout = {"name": NAMES[name], "description": described, "group": section}
                layout.update({k: v for k, v in had.items() if k not in layout})
                for key, example in OLD_EXAMPLES.get(name, {}).items():
                    layout["slots"][key]["example"] = example
                for key, changes in OLD_SLOTS.get(name, {}).items():
                    layout["slots"][key].update(changes)
                for key, changes in OLD_SLOTS_IN.get(geometry, {}).get(name, {}).items():
                    layout["slots"][key].update(changes)
            checked(theme, name, layout["slots"], name in NEW)
            out[name] = layout
    missing = set(old) - set(out)
    assert not missing, f"{theme}'s layouts the library leaves out: {sorted(missing)}"
    names = [layout["name"] for layout in out.values()]
    assert len(set(names)) == len(names), f"{theme}: two layouts share a name"
    for name, layout in out.items():
        said = layout["description"]
        assert "`" not in said and not any(c.isdigit() and said[i - 1] in "EW" for i, c in enumerate(said) if i), (
            f"{theme}'s {name}: its description names a slot or a lint code"
        )
    return out


# A slide's number and footer (ADR-0028): at the foot of the slide, in the theme's `label`, the
# footer from the left across eight columns and the number at the right in the last.
FURNITURE = {
    "number": {"col": [12, 12], "role": "label", "align": "end"},
    "footer": {"col": [1, 8], "role": "label"},
}


def furniture(layouts):
    """The theme's furniture: left off its titles and dividers, as a presentation app leaves a slide
    number off a title slide, and off a layout whose picture bleeds to the foot of the slide, where
    the number would stand on the picture."""
    left = []
    for name, layout in layouts.items():
        bleeds = any(s.get("bleed") and cells(s)[1][1] == 12 for s in layout["slots"].values() if "row" in s)
        if layout.get("group") == "Titles" or bleeds:
            left.append(name)
    return {**FURNITURE, "except": left}


def compact(v):
    """JSON as the theme files set a layout: on one line, a space inside each brace."""
    if isinstance(v, dict):
        if not v:
            return "{}"
        return "{ " + ", ".join(f"{json.dumps(k, ensure_ascii=False)}: {compact(x)}" for k, x in v.items()) + " }"
    if isinstance(v, list):
        return "[" + ", ".join(compact(x) for x in v) + "]"
    return json.dumps(v, ensure_ascii=False)


def written(text, layouts):
    """The theme file `text` with its layouts block set to `layouts`, one to a line, and its
    furniture block set from them, last; the rest as it was."""
    lines = text.split("\n")
    start = lines.index('  "layouts": {')
    end = next(i for i in range(start + 1, len(lines)) if lines[i] in ("  },", "  }"))
    width = max(len(json.dumps(n, ensure_ascii=False)) for n in layouts) + 2
    body = []
    names = list(layouts)
    for i, name in enumerate(names):
        key = (json.dumps(name, ensure_ascii=False) + ":").ljust(width)
        body.append(f"    {key}{compact(layouts[name])}{',' if i < len(names) - 1 else ''}")
    lines = lines[: start + 1] + body + lines[end:]
    # The furniture, the theme's last key, as the model orders a theme's keys.
    if '  "furniture": {' in lines:
        at = lines.index('  "furniture": {')
        lines = lines[: at - 1] + [lines[at - 1].rstrip(",")] + lines[next(i for i in range(at, len(lines)) if lines[i] == "  }") + 1 :]
    close = max(i for i, line in enumerate(lines) if line == "}")
    held = furniture(layouts)
    block = ['  "furniture": {'] + [
        f"    {json.dumps(k)}: {compact(v)}{',' if i < len(held) - 1 else ''}" for i, (k, v) in enumerate(held.items())
    ] + ["  }"]
    lines = lines[: close - 1] + [lines[close - 1] + ","] + block + lines[close:]
    return "\n".join(lines)


def olds(listed):
    """The layouts each theme had, by its first file: its own, and where its file lacks one, as a
    new theme's does, the first file of its geometry family's (PLAN T.0a)."""
    first = {}
    out = {}
    for slug, geometry, paths in listed:
        own = json.loads(paths[0].read_text())["layouts"]
        first.setdefault(geometry, own)
        out[slug] = {**first[geometry], **own}
    return out


def main():
    check = "--check" in sys.argv[1:]
    listed = themes(json.loads(REGISTRY.read_text()))
    had = olds(listed)
    offered = {n for names in ORDER.values() for n in names}
    assert set(NAMES) == offered, f"named but not offered, or offered but not named: {sorted(set(NAMES) ^ offered)}"
    assert set(DESCRIPTIONS) <= offered, sorted(set(DESCRIPTIONS) - offered)
    stale = []
    for slug, geometry, paths in listed:
        layouts = library(slug, had[slug], geometry)
        for path in paths:
            text = path.read_text()
            out = written(text, layouts)
            json.loads(out)
            if out != text:
                stale.append(path)
                if not check:
                    path.write_text(out)
    count = len(next(iter(ORDER.values()))) and sum(len(v) for v in ORDER.values())
    if check and stale:
        sys.exit(f"the layouts are not what scripts/build_layouts.py writes: {', '.join(str(p.relative_to(ROOT)) for p in stale)}")
    print(f"{count} layouts in {len(SECTIONS)} sections; " + ("each theme as written" if check else f"wrote {len(stale)} theme files"))


if __name__ == "__main__":
    main()
