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


    def test_the_four_name_every_italic_and_keep_nothing_whole(self):
        for slug in ("dusk", "daybreak", "ember", "salon"):
            self.assertEqual(themes.faces(themes.themed(slug, SPECS[slug], BASE)), [], slug)

    def test_only_a_family_whose_script_sets_no_italic_is_upright_by_design(self):
        theme = themes.themed("basel", spec(), BASE)
        body = theme["type"]["families"]["body"]
        del body["italic"]
        self.assertRegex(" ".join(themes.faces(theme)), "`body` \\(Inter\\) names no italic")
        body["upright"] = True
        self.assertRegex(" ".join(themes.faces(theme)), "pinned table has its italic")
        # Noto Sans JP, which has none and sets Japanese: upright by design.
        body.update(family="Noto Sans JP", file="fonts/NotoSansJP-VF.ttf")
        self.assertEqual(themes.faces(theme), [])
        body["italic"] = {"file": "fonts/NotoSansJP-VF.ttf"}
        self.assertRegex(" ".join(themes.faces(theme)), "upright by design and names an italic")

    def test_a_family_whose_name_is_reserved_is_kept_whole_and_no_other(self):
        theme = themes.themed("basel", spec(), BASE)
        body = theme["type"]["families"]["body"]
        del body["italic"]
        body.update(family="Pretendard", file="fonts/Pretendard-VF.ttf", upright=True)
        self.assertRegex(" ".join(themes.faces(theme)), "license reserves \\(Pretendard\\): keep it `whole`")
        body["whole"] = True
        self.assertEqual(themes.faces(theme), [])
        # Noto Sans JP reserves "Source", which its name does not use: subset as any other.
        body.update(family="Noto Sans JP", file="fonts/NotoSansJP-VF.ttf")
        self.assertRegex(" ".join(themes.faces(theme)), "reserves no name it uses")


class Carried(unittest.TestCase):
    """Where scaena-ops' build finds each font a theme names (PLAN T.0c)."""

    def test_the_fours_fonts_come_from_git(self):
        theme = themes.themed("basel", spec(), BASE)
        sources = themes.font_sources([theme])
        self.assertEqual([p for p, _, _ in sources], themes.font_files([theme]))
        self.assertEqual(sources[0], ("fonts/Fraunces-VF.ttf", "fraunces", "docs/examples/fonts/Fraunces-VF.ttf"))

    def test_a_family_git_lacks_comes_from_the_cache_by_its_pin(self):
        theme = themes.themed("basel", spec(), BASE)
        body = theme["type"]["families"]["body"]
        body.update(family="Geist", file="fonts/Geist[wght].ttf", italic={"file": "fonts/Geist-Italic[wght].ttf"})
        pins = themes.pinned_families()["geist"]
        sources = {p: (k, s) for p, k, s in themes.font_sources([theme])}
        self.assertEqual(sources["fonts/Geist[wght].ttf"], ("geist", "sha256:" + pins["upright"][1]))
        self.assertEqual(sources["fonts/Geist-Italic[wght].ttf"], ("geist", "sha256:" + pins["italic"][1]))
        self.assertIn("fonts/Geist-Italic[wght].ttf\tgeist\tsha256:", themes.carried_tsv([theme]))

    def test_a_face_the_table_pins_none_of_is_refused_naming_its_family(self):
        theme = themes.themed("basel", spec(), BASE)
        theme["type"]["families"]["body"].update(family="Pretendard", file="fonts/Pretendard.otf", italic={"file": "fonts/Pretendard-Italic.otf"})
        with self.assertRaisesRegex(themes.Refused, "Pretendard's italic"):
            themes.font_sources([theme])


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
        self.assertEqual(themes.AS_THEY_SHIP, {"dusk", "daybreak", "ember", "salon"})
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



class Backgrounds(unittest.TestCase):
    """The background presets (PLAN 2.147): each the base names, in every theme, its palette worked
    out from the theme's colors where the spec gives none, and held to read under a title's text."""

    def palettes(self, slug, s):
        theme = themes.themed(slug, s, BASE)
        return theme["shaders"]["presets"], theme["shaders"]["palettes"], s["colors"]

    def test_every_theme_takes_every_background_and_its_palette(self):
        for slug, s in [*SPECS.items(), ("basel", spec())]:
            presets, palettes, _ = self.palettes(slug, s)
            self.assertEqual(list(presets), list(BASE["shaders"]["presets"]), slug)
            for name, preset in presets.items():
                self.assertIn(preset["palette"], palettes, f"{slug}'s {name}")

    def test_each_color_worked_out_reads_under_the_text(self):
        for slug, s in [*SPECS.items(), ("basel", spec())]:
            _, palettes, colors = self.palettes(slug, s)
            for name in ("aurora", "lagoon", "sweep", "haze", "dust"):
                for c in palettes[name]:
                    for text, bar in themes.OVER:
                        ratio = themes.contrast(themes.rgb(colors[text]), themes.rgb(c))
                        self.assertGreaterEqual(ratio, bar * themes.ROOM - 0.02, f"{slug}'s {name} {c} under {text}")

    def test_the_water_lies_under_its_light(self):
        for slug, s in [*SPECS.items(), ("basel", spec())]:
            _, palettes, colors = self.palettes(slug, s)
            water, *light = palettes["lagoon"]
            lum = lambda c: themes.luminance(themes.rgb(c))  # noqa: E731
            # The light rises from the water: on a light theme a tint of the accent under the paper,
            # its light the paler; on a dark one a deep shade of it, about the paper's own depth.
            self.assertTrue(all(lum(l) > lum(water) for l in light), f"{slug}: {palettes['lagoon']}")
            if s["mode"] == "light":
                self.assertLess(lum(water), lum(colors["paper"]), slug)
            else:
                self.assertLess(themes.contrast(themes.rgb(water), themes.rgb(colors["paper"])), 1.2, slug)

    def test_a_dark_themes_light_stays_within_three_to_one_of_its_water(self):
        for slug in ("dusk", "ember"):
            _, palettes, colors = self.palettes(slug, SPECS[slug])
            water, *light = palettes["lagoon"]
            for c in light:
                ratio = themes.contrast(themes.rgb(c), themes.rgb(water))
                self.assertLessEqual(ratio, themes.CALM + 1e-6, f"{slug}'s {c} on its water {water}")
                self.assertGreater(ratio, 2.5, f"{slug}'s {c} still shows on its water {water}")
        # A light theme's light is left as it was: it only adds contrast to dark words.
        _, palettes, _ = self.palettes("salon", SPECS["salon"])
        self.assertEqual(palettes["lagoon"][-1], "#FFFFFF")

    def test_a_specs_own_palette_wins(self):
        mine = ["#000000", "#FFFFFF"]
        s = spec(shaders={"palettes": {"lagoon": mine}})
        _, palettes, _ = self.palettes("basel", s)
        self.assertEqual(palettes["lagoon"], mine)
        self.assertNotEqual(palettes["aurora"], mine)

if __name__ == "__main__":
    unittest.main()
