#!/usr/bin/env python3
"""The shipped themes' library of layouts (PLAN 3.30, 3.31), written into Dusk, Daybreak, Ember, and Gallery.

Each layout is a slide a person starts from Add Slide: slots on the theme's 12 × 12 grid, each
saying what goes in it. This script holds the library's catalog: the sections it is offered in,
in order, and every layout in each: the 47 the themes had, and those added since. It writes each
theme file's `layouts`, and nothing else in it.

- **The layouts the themes had** keep the slots each theme gave them: Ember's sit where its design
  wants them, and Dusk's and Daybreak's are one. This script puts each in its section and gives
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
THEMES = {
    "dusk": [ROOT / "docs/examples/themes/dusk.theme.json", ROOT / "docs/examples/authorability/themes/dusk.theme.json"],
    "daybreak": [ROOT / "docs/examples/authorability/themes/daybreak.theme.json"],
    "ember": [ROOT / "docs/examples/themes/ember.theme.json"],
    # Gallery (PLAN 2.102) takes Daybreak's layouts, on the same grid.
    "gallery": [ROOT / "docs/examples/themes/gallery.theme.json"],
}

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
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "label", w, inset=24) for o, (c, r), w in zip(ORDINALS, [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))], ["Strengths", "Weaknesses", "Opportunities", "Threats"])},
    **{o: slot(c, (r[0] + 2, r[1]), "caption", {"bullet": w}, inset=24) for o, (c, r), w in zip(ORDINALS, [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))], [["What we do well", "Our edge"], ["Where we fall short", "What we lack"], ["What opens up", "Who needs us"], ["What could hurt us", "Who competes"]])},
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
    # A row clear of the headline, so a headline of two lines stands clear of the numbers.
    **{f"{o}-number": slot(c, (4, 5), "numeral", str(i + 1)) for i, (o, c) in enumerate(zip(ORDINALS, THIRDS))},
    **{f"{o}-title": slot(c, (6, 7), "title", w) for o, c, w in zip(ORDINALS, THIRDS, ["Sign up", "Set it up", "Go"])},
    **{o: slot(c, (8, 10), "body", "What happens, in a line") for o, c in zip(ORDINALS, THIRDS)},
}, ember={**{f"{o}-number": (c, (4, 8)) for o, c in zip(ORDINALS, THIRDS)}, **{f"{o}-title": (c, (9, 10)) for o, c in zip(ORDINALS, THIRDS)}, **{o: (c, (11, 12)) for o, c in zip(ORDINALS, THIRDS)}})

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
    **{f"{lane}-{o}": slot(c, r, "caption", w, align={"y": "center"}) for lane, r, ws in zip(["does", "thinks", "feels"], [(4, 5), (6, 7), (8, 9)], [["Hears of it", "Signs up", "Comes back"], ["Is it for me?", "This is easy", "I rely on it"], ["Curious", "Relieved", "At home"]]) for o, c, w in zip(ORDINALS, [(4, 6), (7, 9), (10, 12)], ws)},
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
    "note": slot((7, 12), (9, 10), "caption", "Six months, by month"),
}, ember={"number": ((1, 5), (4, 8)), "claim": ((1, 5), (10, 12)), "main": ((7, 12), (4, 10)), "note": ((7, 12), (11, 12))})

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
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "title", "A question?", inset=24) for o, (c, r) in zip(ORDINALS, FAQ)},
    **{o: slot(c, (r[0] + 2, r[1]), "body", "Its answer, in a line.", inset=24) for o, (c, r) in zip(ORDINALS, FAQ)},
})

GLOSSARY = [((1, 6), (3, 6)), ((7, 12), (3, 6)), ((1, 6), (7, 10)), ((7, 12), (7, 10))]
layout("Editorial", "glossary", "Terms and what they mean, set as a dictionary sets them.", {
    "kicker": KICKER,
    "header": header("Terms"),
    **{f"{o}-title": slot(c, (r[0], r[0] + 1), "title", "A term") for o, (c, r) in zip(ORDINALS, GLOSSARY)},
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

OLD_SLOTS_IN = {
    "dusk": {"stat": {"number": {"row": [3, 7]}}, "timeline": timeline_rows(3), "title": TITLE_KICKER},
    "daybreak": {"stat": {"number": {"row": [3, 7]}}, "timeline": timeline_rows(3), "title": TITLE_KICKER},
    "gallery": {"title": TITLE_KICKER},
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
    "team-four": "Team of Four", "team-six": "Team of Six", "org-chart": "Org Chart", "speaker": "Speaker",
    "two-quotes": "Two Quotes",
    # Editorial
    "definition": "Definition", "letter": "Letter", "memo": "Memo", "receipt": "Receipt",
    "chyron": "Lower Third", "headline": "Front Page", "faq": "FAQ", "glossary": "Glossary",
    "magazine": "Magazine Spread",
    # Closing
    "closing": "Closing", "next-steps": "Next Steps", "the-ask": "The Ask", "qa": "Q&A", "contact": "Contact",
    "takeaways": "Takeaways", "call-to-action": "Call to Action",
}

# The descriptions of the layouts the themes had, where one named what a person never reads in an
# editor's window, a slot or a lint code (PLAN 3.18): by theme, `*` for each.
DESCRIPTIONS = {
    "title": {"*": "The opening: the talk's title, a line on what it's about, and a picture beside them."},
    "figure": {
        "dusk": "A headline, the figure that shows it, and a note on it, each in its own rows: the data on the plain surface, with no picture behind it.",
        "daybreak": "A headline, the figure that shows it, and a note on it, each in its own rows: the data on the plain surface, with no picture behind it.",
    },
    "narrow-figure": {"*": "A headline, a chart of two or three bars, and a note on it beside the chart."},
    "split": {"*": "Words, and the chart or table beside them: the data on the plain surface, with no picture behind it."},
    "full": {
        "dusk": "A headline over one thing that fills the grid, a picture, a chart, or a table, and a note on it.",
        "daybreak": "A headline over one thing that fills the grid, a picture, a chart, or a table, and a note on it.",
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


def library(theme, old):
    """The theme's layouts, in the library's order: those it had, put in their sections and given
    their examples, and those added here, Ember's made from Dusk's."""
    out = {}
    for section in SECTIONS:
        for name in ORDER[section]:
            if name in NEW:
                spec = NEW[name]
                slots = spec["slots"]
                if theme == "ember":
                    slots = ember_slots(name, slots, spec["ember"])
                layout = {"name": NAMES[name], "description": spec["description"], "group": section, "slots": slots}
            else:
                assert name in old, f"{theme} has no layout `{name}`"
                had = json.loads(json.dumps(old[name]))
                told = DESCRIPTIONS.get(name, {})
                described = told.get(theme, told.get("*", had.get("description")))
                layout = {"name": NAMES[name], "description": described, "group": section}
                layout.update({k: v for k, v in had.items() if k not in layout})
                for key, example in OLD_EXAMPLES.get(name, {}).items():
                    layout["slots"][key]["example"] = example
                for key, changes in OLD_SLOTS.get(name, {}).items():
                    layout["slots"][key].update(changes)
                for key, changes in OLD_SLOTS_IN.get(theme, {}).get(name, {}).items():
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


def main():
    check = "--check" in sys.argv[1:]
    listed = {n for names in ORDER.values() for n in names}
    assert set(NAMES) == listed, f"named but not offered, or offered but not named: {sorted(set(NAMES) ^ listed)}"
    assert set(DESCRIPTIONS) <= listed, sorted(set(DESCRIPTIONS) - listed)
    stale = []
    for theme, paths in THEMES.items():
        for path in paths:
            text = path.read_text()
            old = json.loads(text)["layouts"]
            out = written(text, library(theme, old))
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
