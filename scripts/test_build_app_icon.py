"""The apps' icons and the iPad's accent, as the apps carry them: each size the Mac's icon set needs,
the iPad's and the remote's catalogs, the keys that name them, and an accent New Deck reads on."""

import json
import plistlib
import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_app_icon as icon  # noqa: E402

ROOT = icon.ROOT


def header(path):
    """A PNG's width, height, and color type, from its header."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n" and data[12:16] == b"IHDR", f"{path} is not a PNG"
    width, height, _, color = struct.unpack(">IIBB", data[16:26])
    return width, height, color


def accent(catalog):
    """A catalog's accent: its color, and its color in dark mode, each (red, green, blue)."""
    colors = json.loads((catalog / "AccentColor.colorset/Contents.json").read_text())["colors"]

    def rgb(entry):
        c = entry["color"]["components"]
        return tuple(int(c[k], 16) for k in ("red", "green", "blue"))

    light = [rgb(e) for e in colors if "appearances" not in e]
    dark = [rgb(e) for e in colors if any(a["value"] == "dark" for a in e.get("appearances", []))]
    return light[0], dark[0]


class TheMacsIcon(unittest.TestCase):
    def test_the_icon_set_holds_each_size_iconutil_reads(self):
        for name, px in icon.MAC_SIZES.items():
            self.assertEqual(header(icon.ICONSET / name), (px, px, 6), name)
        self.assertEqual(sorted(p.name for p in icon.ICONSET.iterdir()), sorted(icon.MAC_SIZES))

    def test_the_app_names_the_icon_its_build_makes(self):
        plist = plistlib.loads((ROOT / "apps/mac/Scaena/Info.plist").read_bytes())
        self.assertEqual(plist["CFBundleIconFile"], "AppIcon")
        made = 'iconutil -c icns -o "$app/Contents/Resources/AppIcon.icns" "$root/apps/mac/Scaena/AppIcon.iconset"'
        for script in ("apps/mac/build-app.sh", "apps/mac/window.sh"):
            self.assertIn(made, (ROOT / script).read_text(), script)

    def test_the_body_is_clear_outside_macos_grid(self):
        rows = icon.unpng((icon.ICONSET / "icon_32x32@2x.png").read_bytes())
        # 100 of 1024 px clear each side: 6 of 64.
        for y in (0, 5, 58, 63):
            self.assertTrue(all(px[3] == 0 for px in rows[y]), f"row {y}")
        self.assertEqual(rows[32][32][3], 255)


class TheIPadsIcons(unittest.TestCase):
    def test_each_catalog_holds_an_opaque_square_icon_and_the_accent(self):
        for catalog in (icon.CATALOG, icon.REMOTE_CATALOG):
            listed = json.loads((catalog / "AppIcon.appiconset/Contents.json").read_text())["images"]
            self.assertEqual(
                listed, [{"filename": "AppIcon.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"}]
            )
            # RGB, no alpha: iPadOS rounds the corners itself.
            self.assertEqual(header(catalog / "AppIcon.appiconset/AppIcon.png"), (1024, 1024, 2), catalog)
            self.assertEqual(accent(catalog), (icon.ACCENT_LIGHT, icon.ACCENT_DARK), catalog)

    def test_the_project_compiles_each_catalogs_icon_and_accent(self):
        project = (ROOT / "apps/ipad/project.yml").read_text()
        targets = {
            name: project.split(f"\n  {name}:\n", 1)[1].split("\n  # ", 1)[0] for name in ("ScaenaApp", "RemoteApp")
        }
        self.assertIn("- path: Assets.xcassets", targets["ScaenaApp"])
        self.assertIn("- path: Remote\n", targets["RemoteApp"])
        for name, target in targets.items():
            self.assertIn("ASSETCATALOG_COMPILER_APPICON_NAME: AppIcon", target, name)
            self.assertIn("ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME: AccentColor", target, name)

    def test_a_decks_icon_is_the_apps_on_a_page(self):
        for path in ("apps/mac/Scaena/Info.plist", "apps/ipad/Info.plist"):
            plist = plistlib.loads((ROOT / path).read_bytes())
            for kind in plist["UTExportedTypeDeclarations"]:
                self.assertEqual(
                    kind.get("UTTypeIcons"), {"UTTypeIconText": "deck"}, f"{path}: {kind['UTTypeIdentifier']}"
                )


class TheAccent(unittest.TestCase):
    def test_new_deck_reads_on_it_as_the_launch_screen_draws_it(self):
        # CI's screenshot of the launch screen (PR #366): New Deck on the system's blue, its words
        # that blue lifted by 130 a channel, which read at 3 to 1.
        blue = (0x00, 0x88, 0xFE)
        self.assertEqual(icon.lifted(blue), (0x82, 0xFF, 0xFF))
        self.assertLess(icon.contrast(icon.lifted(blue), blue), 3.0)
        # The accent: the words lifted over it, or white, read at 4.5 to 1 or more.
        self.assertGreaterEqual(icon.contrast(icon.lifted(icon.ACCENT_LIGHT), icon.ACCENT_LIGHT), 4.5)
        self.assertGreaterEqual(icon.contrast((255, 255, 255), icon.ACCENT_LIGHT), 4.5)

    def test_it_is_a_shade_of_the_dusk_accent(self):
        for shade in (icon.ACCENT_LIGHT, icon.ACCENT_DARK):
            k = shade[0] / icon.ACCENT[0]
            for c, d in zip(shade, icon.ACCENT):
                self.assertAlmostEqual(c, d * k, delta=1)

    def test_in_dark_mode_it_reads_as_words_on_the_dark_backgrounds(self):
        for background in ((0, 0, 0), (0x1C, 0x1C, 0x1E)):
            self.assertGreaterEqual(icon.contrast(icon.ACCENT_DARK, background), 4.5, background)


if __name__ == "__main__":
    unittest.main()
