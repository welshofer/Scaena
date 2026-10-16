"""The themes written from their specs (PLAN T.0a, ADR-0031): a spec in, the file out; a new
theme's colors held to the bars; the four held to their files as they ship."""

import copy
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_themes as themes  # noqa: E402

REGISTRY, BASE, SPECS = themes.read()

# Basel's colors, as ADR-0031 lists them: a theme of batch 1 on the four's fonts.
BASEL = {
    "ink": "#111111",
    "paper": "#F5F5F2",
    "paper-2": "#DEDED9",
    "accent": "#B80F2A",
    "accent-2": "#1F3FBF",
    "muted": "#474747",
    "line": "#C9C9C3",
}


def spec(**over) -> dict:
    """A new theme's spec: Daybreak's, named Basel, in Basel's colors."""
    s = copy.deepcopy(SPECS["daybreak"])
    s.update(name="Basel", description="Swiss: flush-left type, red the only color.", colors=dict(BASEL))
    s.update(over)
    return s


class FromSpec(unittest.TestCase):
    def test_a_spec_makes_a_theme_file_on_the_base(self):
        theme = themes.themed("basel", spec(), BASE)
        self.assertEqual(theme["name"], "Basel")
        self.assertEqual(theme["tokens"]["color"], BASEL)
        self.assertEqual(theme["motion"], BASE["motion"])
        self.assertEqual(theme["type"]["roles"], BASE["voices"]["dusk"]["roles"])
        # Each key where the four put it; the base's empty places left out.
        self.assertEqual(list(theme), [k for k, v in BASE.items() if k not in ("voices", "layouts") and v is not None or k in ("name", "description")])
        self.assertNotIn("null", json.dumps(theme))
        text = themes.written(theme, None, BASE)
        out = json.loads(text)
        self.assertEqual(list(out)[list(out).index("grid") + 1], "layouts")
        self.assertEqual(out["layouts"], {})
        self.assertNotIn("furniture", out)

    def test_a_specs_code_look_lands_after_the_density(self):
        code = {"role": "code", "tab": 4}
        theme = themes.themed("basel", spec(code=code), BASE)
        self.assertEqual(theme["code"], code)
        self.assertEqual(list(theme)[-3:], ["density", "code", "pictures"])

    def test_a_specs_frame_takes_the_place_of_the_bases_none(self):
        self.assertEqual(themes.themed("basel", spec(), BASE)["pictures"], {"frame": "none"})
        frame = {"mat": {"color": "surface", "width": 24}, "keyline": {"color": "line", "width": 1}}
        theme = themes.themed("basel", spec(pictures={"frame": frame}), BASE)
        self.assertEqual(theme["pictures"], {"frame": frame})

    def test_a_role_delta_changes_only_what_it_names_and_null_takes_a_key_away(self):
        theme = themes.themed("basel", spec(roles={"display": {"size": 160, "opsz": None}}, stroke={"thick": 5}), BASE)
        dusk = BASE["voices"]["dusk"]["roles"]["display"]
        display = theme["type"]["roles"]["display"]
        self.assertEqual(display["size"], 160)
        self.assertNotIn("opsz", display)
        self.assertEqual({k: v for k, v in display.items() if k != "size"}, {k: v for k, v in dusk.items() if k not in ("size", "opsz")})
        self.assertEqual(theme["tokens"]["stroke"]["thick"], 5)
        self.assertEqual(theme["tokens"]["stroke"]["thin"], BASE["tokens"]["stroke"]["thin"])

    def test_a_spec_holding_what_a_spec_does_not_is_refused(self):
        with self.assertRaisesRegex(themes.Refused, "motion"):
            themes.themed("basel", spec(motion={}), BASE)
        with self.assertRaisesRegex(themes.Refused, "voice"):
            themes.themed("basel", spec(voice="loud"), BASE)
        with self.assertRaisesRegex(themes.Refused, "dark theme"):
            themes.themed("basel", spec(mode="dark"), BASE)


    def test_a_family_the_pinned_table_lacks_is_refused(self):
        theme = themes.themed("basel", spec(), BASE)
        self.assertEqual(themes.fonts_of(theme), ["fraunces", "inter", "jetbrains-mono"])
        theme["type"]["families"]["display"]["family"] = "Comic Sans"
        with self.assertRaisesRegex(themes.Refused, "Comic Sans"):
            themes.fonts_of(theme)


class Bars(unittest.TestCase):
    def test_basel_meets_every_bar(self):
        self.assertEqual(themes.missed("basel", spec()), [])

    def test_a_new_theme_under_each_bar_is_refused_naming_the_color(self):
        under = {
            # The ink under 7:1 on paper-2.
            "ink": ("#4A4A4A", "ink #4A4A4A reads"),
            # Quieter text under 4.5:1.
            "muted": ("#8A8A8A", "muted #8A8A8A reads"),
            # The accent under 4.5:1, so the paper on it too.
            "accent": ("#E07A88", "paper #F5F5F2 reads"),
            # The second accent under 3:1.
            "accent-2": ("#A9B5E6", "accent-2 #A9B5E6 reads"),
            # A line under 1.3:1 on the page.
            "line": ("#E8E8E4", "line #E8E8E4 reads"),
            # Cards under 1.2:1 off it.
            "paper-2": ("#EDEDE9", "paper-2 #EDEDE9 reads"),
        }
        for color, (value, said) in under.items():
            s = spec()
            s["colors"][color] = value
            with self.subTest(color=color):
                with self.assertRaises(themes.Refused) as refused:
                    themes.themed("basel", s, BASE)
                self.assertIn(said, str(refused.exception))
                self.assertIn("Oklab lightness", str(refused.exception))

    def test_the_high_contrast_pair_holds_every_text_color_to_seven(self):
        s = spec(contrast="high")
        s["colors"]["muted"] = "#5E5E5E"  # 5.9:1 on the page: enough for most, not for Beacon's bar
        self.assertEqual(themes.missed("basel", spec(colors=s["colors"])), [])
        self.assertTrue(any("muted" in m and "under 7:1" in m for m in themes.missed("basel", s)))

    def test_a_lightness_that_meets_the_bar_meets_it(self):
        need = themes.lightness_for("#8A8A8A", BASEL["paper-2"], 4.5)
        _, a, b = themes.oklab(themes.rgb("#8A8A8A"))
        self.assertGreaterEqual(themes.contrast(themes.from_oklab(need, a, b), themes.rgb(BASEL["paper-2"])), 4.5)


class TheFour(unittest.TestCase):
    def test_the_generator_writes_the_four_as_they_ship(self):
        out, wrong = themes.build(REGISTRY, BASE, SPECS)
        self.assertEqual(wrong, [])
        for entry in REGISTRY["themes"]:
            for f in entry["files"]:
                path = themes.ROOT / f
                self.assertEqual(out[path], path.read_text(encoding="utf-8"), f)

    def test_the_exemption_is_the_four_and_each_still_needs_it(self):
        self.assertEqual(themes.AS_THEY_SHIP, {"dusk", "daybreak", "ember", "gallery"})
        self.assertLessEqual(themes.AS_THEY_SHIP, {e["slug"] for e in REGISTRY["themes"]})
        for slug in themes.AS_THEY_SHIP:
            self.assertTrue(themes.missed(slug, SPECS[slug]), f"{slug} meets every bar: lift its exemption (PLAN T.6)")

    def test_a_new_theme_in_the_fours_colors_is_refused(self):
        with self.assertRaisesRegex(themes.Refused, "paper-2 #17171D reads 1.06:1"):
            themes.themed("nightfall", copy.deepcopy(SPECS["dusk"]), BASE)

    def test_the_registry_copies_each_spec(self):
        for entry in REGISTRY["themes"]:
            s = SPECS[entry["slug"]]
            for k in ("description", "mode", "colors", "voice"):
                self.assertEqual(entry[k], s[k], f"{entry['slug']}'s {k}")


if __name__ == "__main__":
    unittest.main()
