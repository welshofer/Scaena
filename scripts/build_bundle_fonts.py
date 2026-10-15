#!/usr/bin/env python3
"""Build and check the fonts of the benchmark and example bundles (SPEC §15, PLAN 0.14, 1.2).

The same pinned google/fonts commit, sha256 verification, and subset options as the
torture deck's fonts (scripts/build_torture_fonts.py, whose helpers this reuses).
Each font is subset to every character its deck uses plus ASCII, Latin-1, Latin
Extended-A, and common punctuation, which is what saving a bundle will do (SPEC §3.1).
The example decks are bundles like any other: `scaena validate` and `scaena render`
work on them as they stand.

Check (no network; CI runs this): no font is named by a name its copyright reserves, and
every character of every text node's text, in every state, maps in its role's own family,
under each theme the bundle uses. A
benchmark must also set text in every family it ships: one that silently fell back
would time the wrong font. Every `SOURCES.md` in docs/ and tests/ answers for the files beside
it: a row for each font and picture, and each row's sha256 the file's (PLAN T.0b).

Fetch: every family a theme names (FAMILIES, ADR-0031), each file by its sha256, into a cache
CI keeps; `--offline` reads the cache alone and fails on what it lacks.

    pip install fonttools==4.66.1
    python3 scripts/build_bundle_fonts.py           # rebuild fonts, then check
    python3 scripts/build_bundle_fonts.py --cache ~/.cache/scaena-fonts
    python3 scripts/build_bundle_fonts.py --only b2 --only b3   # rebuild two, check all
    python3 scripts/build_bundle_fonts.py --check   # check only
    python3 scripts/build_bundle_fonts.py --fetch   # every family into ~/.cache/scaena-fonts
    python3 scripts/build_bundle_fonts.py --fetch --offline   # the cache holds every family
"""

import argparse
import io
import json
import sys
import unicodedata
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_torture_fonts as torture  # noqa: E402

ROOT = torture.ROOT
# Every family a theme names (ADR-0031's The fonts, PLAN T.0b), by the key a theme names it by:
# where its files are, their sha256s, its italic, its axes, and the scripts it draws. Each is
# google/fonts' at the shipped fonts' commit but Pretendard, which google/fonts does not hold.
# A static family carries the weight its themes set and that weight's italic. `--fetch` fills a
# cache with them by sha256; the four themes' eight files stay in git, as subsets beside the
# example decks, and their pins below are these.
GOOGLE_FONTS = ("google/fonts", torture.COMMIT)
PRETENDARD = ("orioncactus/pretendard", "v1.3.9")
FAMILIES = {
    "alegreya": {
        "name": "Alegreya",
        "upright": (
            "ofl/alegreya/Alegreya[wght].ttf",
            "ba5564634b93a8f8ba57b48cd4f1ae7417d2b4656fbac779028679b00de3cf12",
        ),
        "italic": (
            "ofl/alegreya/Alegreya-Italic[wght].ttf",
            "fa915eec76227935dc5fb678953c94b71287c360928013cfdb441dfe52f5a391",
        ),
        "license": ("ofl/alegreya/OFL.txt", "f6f60d5d4cf4f4b1fc4e41353c897a2f5a16e6396c0cd8fa8bdfd2f4586a9a68"),
        "axes": {"wght": (400, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "alegreya-sans": {
        "name": "Alegreya Sans",
        "upright": (
            "ofl/alegreyasans/AlegreyaSans-Bold.ttf",
            "a3055a1893759bdbd7504bb22abc583769e7974c49353176eac0b03792c9fb8e",
        ),
        "italic": (
            "ofl/alegreyasans/AlegreyaSans-BoldItalic.ttf",
            "6a5550f0e4c3021e7e2e83ac4814e8c649a0fe20385489ce7609f00802097b4d",
        ),
        "license": ("ofl/alegreyasans/OFL.txt", "0677891e6a143f297350d260ad766ad33bfc18ed5fa4f213acf648d6b597ec1a"),
        "axes": {},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "aleo": {
        "name": "Aleo",
        "upright": ("ofl/aleo/Aleo[wght].ttf", "62b6cb49b7b087b341f0307ea3bc871a9690d8947691206ab025d149b8f6ed6f"),
        "italic": (
            "ofl/aleo/Aleo-Italic[wght].ttf",
            "9b4c074d9f3e662ef57bbef82efb7b1bb5f757d579c91c4c0b831319221d6f8c",
        ),
        "license": ("ofl/aleo/OFL.txt", "f13013bb1f4887b67240ec2ec5754ef968a3753ada1e538b598eae375cf74273"),
        "axes": {"wght": (100, 900)},
        "scripts": ("Latin",),
    },
    "amiri": {
        "name": "Amiri",
        "upright": ("ofl/amiri/Amiri-Bold.ttf", "cfccb794268e7d573d857e6d6a67f89cf8a053e8ffd85dfa0c8ec1bb36fc4827"),
        "italic": (
            "ofl/amiri/Amiri-BoldItalic.ttf",
            "241300e2dcb2c541ac3617872960d99683d261e4f526dc4691d0727e7bef130b",
        ),
        "license": ("ofl/amiri/OFL.txt", "72de68e5954f4fdd24702292ef5a32f003ca960ec9330dc86e5eefb5dffb9b22"),
        "axes": {},
        "scripts": ("Latin", "Arabic"),
    },
    "archivo": {
        "name": "Archivo",
        "upright": (
            "ofl/archivo/Archivo[wdth,wght].ttf",
            "0e094a7d3c7c4c25cf1310c4b30014f1dae9332220b1c2c88f4fa996f0b05053",
        ),
        "italic": (
            "ofl/archivo/Archivo-Italic[wdth,wght].ttf",
            "305d13b4eb80e62f7d517f78d7c045250d977552a420f2dbed906f314e761305",
        ),
        "license": ("ofl/archivo/OFL.txt", "108b4e57c9c796d3d38d0428ca7ee39de47ad93187302718d9b2d8864b9b716b"),
        "axes": {"wdth": (62, 125), "wght": (100, 900)},
        "scripts": ("Latin",),
    },
    "arimo": {
        "name": "Arimo",
        "upright": ("ofl/arimo/Arimo[wght].ttf", "e43898b143ec826ac8cb4034816458a7047fbe0836558de2a1f8c6223ae3e0ca"),
        "italic": (
            "ofl/arimo/Arimo-Italic[wght].ttf",
            "a80fc54fd0233c1dfe298577c4d00f5ae81d5bb83510975e473c47e699b7f4ed",
        ),
        "license": ("ofl/arimo/OFL.txt", "11cce536cd2f3864d767003af5dcd739e2e15818cf2279b6175edeadd3960992"),
        "axes": {"wght": (400, 700)},
        "scripts": ("Latin", "Cyrillic", "Greek", "Hebrew"),
    },
    "atkinson-hyperlegible-mono": {
        "name": "Atkinson Hyperlegible Mono",
        "upright": (
            "ofl/atkinsonhyperlegiblemono/AtkinsonHyperlegibleMono[wght].ttf",
            "5ce8b1698d1ded7dff2178c1a3ad159470085a58ea239e8b2cb88f4fb4a6f646",
        ),
        "italic": (
            "ofl/atkinsonhyperlegiblemono/AtkinsonHyperlegibleMono-Italic[wght].ttf",
            "e18f523877530e4abe229df6a36d394326254da488a7f5abb9b7b5ea8f780cd4",
        ),
        "license": (
            "ofl/atkinsonhyperlegiblemono/OFL.txt",
            "1ebb31cf7393164f20d10c1d48406cddb5314feff8465531cf1e4ba37e9dd740",
        ),
        "axes": {"wght": (200, 800)},
        "scripts": ("Latin",),
    },
    "atkinson-hyperlegible-next": {
        "name": "Atkinson Hyperlegible Next",
        "upright": (
            "ofl/atkinsonhyperlegiblenext/AtkinsonHyperlegibleNext[wght].ttf",
            "5a455d1cfa099b601ab70751bb9673e8fe1854dc4500c80e1a220d0d75e31745",
        ),
        "italic": (
            "ofl/atkinsonhyperlegiblenext/AtkinsonHyperlegibleNext-Italic[wght].ttf",
            "ce9cffed32742ad2d9238c561a93220385e5934cdc02b8eb4097a50efa957dc6",
        ),
        "license": (
            "ofl/atkinsonhyperlegiblenext/OFL.txt",
            "aca6a428580965d2297d1b718042dd427c2a9443ece3b0d02d758e161e0c4030",
        ),
        "axes": {"wght": (200, 800)},
        "scripts": ("Latin",),
    },
    "b612": {
        "name": "B612",
        "upright": ("ofl/b612/B612-Bold.ttf", "91749541ac7b2c328b58832b7e2c4df809d7e2ba38d62a3a5aa3f8e38b271814"),
        "italic": ("ofl/b612/B612-BoldItalic.ttf", "5911fe0ae69264fb74b2956fc66c616580cd392b47f29335fa68ce5db47a4d2f"),
        "license": ("ofl/b612/OFL.txt", "a815f65bc72d90494b01842d3171f7cb0f9f935e023d9cc260904d221ef1064a"),
        "axes": {},
        "scripts": ("Latin", "Greek"),
    },
    "barlow": {
        "name": "Barlow",
        "upright": (
            "ofl/barlow/Barlow-SemiBold.ttf",
            "86577cb32f8abe3673db53ca0f4221e6856751a4f6730c867e00f720f8bb1fc5",
        ),
        "italic": (
            "ofl/barlow/Barlow-SemiBoldItalic.ttf",
            "277b45fc0b9f066bf77e88e5d147baae8d1ad7441cedee1dc305fc3dc6e84ce7",
        ),
        "license": ("ofl/barlow/OFL.txt", "186d750eb496a4c17a76385f82be6aea2ac1cf2de074a811d63786cf374ea73f"),
        "axes": {},
        "scripts": ("Latin",),
    },
    "barlow-condensed": {
        "name": "Barlow Condensed",
        "upright": (
            "ofl/barlowcondensed/BarlowCondensed-SemiBold.ttf",
            "7b619d14bc2327509a9ef32b0890f709626f7ecc9ff61191c2a4314c5499d2d9",
        ),
        "italic": (
            "ofl/barlowcondensed/BarlowCondensed-SemiBoldItalic.ttf",
            "dc83cbbb7545a117ab9e4c10ed07d35eef6f563b77621a1d357f25a3194987d1",
        ),
        "license": ("ofl/barlowcondensed/OFL.txt", "186d750eb496a4c17a76385f82be6aea2ac1cf2de074a811d63786cf374ea73f"),
        "axes": {},
        "scripts": ("Latin",),
    },
    "bodoni-moda": {
        "name": "Bodoni Moda",
        "upright": (
            "ofl/bodonimoda/BodoniModa[opsz,wght].ttf",
            "550f5e34ee0a828d7941b1fe9bc58b34e5260d3f33a61532e6d0a0114e79a5cf",
        ),
        "italic": (
            "ofl/bodonimoda/BodoniModa-Italic[opsz,wght].ttf",
            "dfff1619f8f6871c6372f8855b67211f9a73b4e93d45aca868cd8f46a48622de",
        ),
        "license": ("ofl/bodonimoda/OFL.txt", "931dfe2e0cd3c9443295f5c1d754b8a8403bf060df50c1d5900267ade83d0046"),
        "axes": {"opsz": (6, 96), "wght": (400, 900)},
        "scripts": ("Latin",),
    },
    "comic-neue": {
        "name": "Comic Neue",
        "upright": (
            "ofl/comicneue/ComicNeue-Bold.ttf",
            "3e7e5fccfd7e0788f317b43312151c1bd5cf058c9697a8d83eac3939050bd61e",
        ),
        "italic": (
            "ofl/comicneue/ComicNeue-BoldItalic.ttf",
            "5c312c2a2fa64eee82f3b87fcfab8f3b12a5e59b043124401d322eb323cfbf16",
        ),
        "license": ("ofl/comicneue/OFL.txt", "7c38a22e5878e60fe423360553e63dd7be23d29f1f60336034935dbfc96e8320"),
        "axes": {},
        "scripts": ("Latin",),
    },
    "cormorant-garamond": {
        "name": "Cormorant Garamond",
        "upright": (
            "ofl/cormorantgaramond/CormorantGaramond[wght].ttf",
            "b20b7d9626dd956b2c5e558692ad328b1f19e3275e2782db4fa07670d83f35e0",
        ),
        "italic": (
            "ofl/cormorantgaramond/CormorantGaramond-Italic[wght].ttf",
            "0f48ea6abb2084537854f7174c470991a463b13036309e3b50a81511611c530d",
        ),
        "license": (
            "ofl/cormorantgaramond/OFL.txt",
            "60700d351cac4650c51f3f9db318d2a420f8b45052dba2715eb5fec41f0f6956",
        ),
        "axes": {"wght": (300, 700)},
        "scripts": ("Latin", "Cyrillic"),
    },
    "cousine": {
        "name": "Cousine",
        "upright": (
            "ofl/cousine/Cousine-Regular.ttf",
            "1da22250675fc4c42fcf3a9736c44bc0570516105331443b663fd5cfbd1412fe",
        ),
        "italic": (
            "ofl/cousine/Cousine-Italic.ttf",
            "ea2a76ae3d0ece9cd59f0d30fdc08dd70e8f5f457beee5b0852a7b50c2286c7c",
        ),
        "license": ("ofl/cousine/OFL.txt", "b81c4d4dc0a9f72c9155e78187316e016e2012a8102468804173dc61468b906d"),
        "axes": {},
        "scripts": ("Latin", "Cyrillic", "Greek", "Hebrew"),
    },
    "dm-sans": {
        "name": "DM Sans",
        "upright": (
            "ofl/dmsans/DMSans[opsz,wght].ttf",
            "8cd08d97e89c24d0aa92edd2f0f4c8ee6195eee9b7c9f154865a58b02f0c1c0d",
        ),
        "italic": (
            "ofl/dmsans/DMSans-Italic[opsz,wght].ttf",
            "22259c0cc8237221b80f44c76ba8d36e6bce3cda72779f5b2773643d499720ae",
        ),
        "license": ("ofl/dmsans/OFL.txt", "9af36190332437f5ecd09974de43c1f7c77a310a996cdd8ceb25628b458840e1"),
        "axes": {"opsz": (9, 40), "wght": (100, 1000)},
        "scripts": ("Latin",),
    },
    "eb-garamond": {
        "name": "EB Garamond",
        "upright": (
            "ofl/ebgaramond/EBGaramond[wght].ttf",
            "ef9512f92f6d579e5dc75af59a5a4b1b8b47d2eda89e00b954d44520e5369027",
        ),
        "italic": (
            "ofl/ebgaramond/EBGaramond-Italic[wght].ttf",
            "bba2c4499c93c9612b90b9825d32b07da52fce2fe57562a1eb6b833553f93c4e",
        ),
        "license": ("ofl/ebgaramond/OFL.txt", "0985066662eb755ed3683ae5482a81a9195b49ce3f7e165cc2388b3dbece7dd7"),
        "axes": {"wght": (400, 800)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "fraunces": {
        "name": "Fraunces",
        "upright": (
            "ofl/fraunces/Fraunces[SOFT,WONK,opsz,wght].ttf",
            "177ff6c0f14e5550a3c624247cd1189611d4eb65d000b14944c63d967958abbb",
        ),
        "italic": (
            "ofl/fraunces/Fraunces-Italic[SOFT,WONK,opsz,wght].ttf",
            "b24448c43702fac4ee856781d461a0dfba8d8e594b6e8e190234b75fed2c0e01",
        ),
        "license": ("ofl/fraunces/OFL.txt", "bdf4c22802eaf804f998195871c6b8938aac2ac14b2d78a8bd66a6f1eced833b"),
        "axes": {"SOFT": (0, 100), "WONK": (0, 1), "opsz": (9, 144), "wght": (100, 900)},
        "scripts": ("Latin",),
    },
    "geist": {
        "name": "Geist",
        "upright": ("ofl/geist/Geist[wght].ttf", "73894e0448cae90a92b6c2f8732b7bb9acb7b94c418bff559dad4a18e1de9659"),
        "italic": (
            "ofl/geist/Geist-Italic[wght].ttf",
            "a464348abe41a6392f18d04c0ac2ad3c26619357ff5fdd76aff3059116f45e62",
        ),
        "license": ("ofl/geist/OFL.txt", "1781d2806a07d91c4edf4740b88449fab7d0eadad53f7c351b94cd4d4eb8c00f"),
        "axes": {"wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic"),
    },
    "gelasio": {
        "name": "Gelasio",
        "upright": (
            "ofl/gelasio/Gelasio[wght].ttf",
            "4daecea457258c9ebeb8bc99ed3fd24353618bfad3ea4b93fa0b5d0468fc04e4",
        ),
        "italic": (
            "ofl/gelasio/Gelasio-Italic[wght].ttf",
            "52559e845a4d33514e5f93bb9ae7dbeae1894a53f2c565a15f18af40cd337c09",
        ),
        "license": ("ofl/gelasio/OFL.txt", "b393cb01867c919b44381512120dc3e4c954c7b47e2035c405f3a324799a4d29"),
        "axes": {"wght": (400, 700)},
        "scripts": ("Latin",),
    },
    "google-sans": {
        "name": "Google Sans",
        "upright": (
            "ofl/googlesans/GoogleSans[GRAD,opsz,wght].ttf",
            "d0a87d835a944b8b40d0e82a5651bb59ab97b936a2aeed5946eb57e7b2a3a90a",
        ),
        "italic": (
            "ofl/googlesans/GoogleSans-Italic[GRAD,opsz,wght].ttf",
            "b6662abd2131cad1b248750149a7f2b644f871ebcc9af7fe4cd1135cd290b6d1",
        ),
        "license": ("ofl/googlesans/OFL.txt", "2b75ef20f13d83a7514aee452c4782c20cdc9ff2dee17600f44d37a06d4fb958"),
        "axes": {"GRAD": (-50, 200), "opsz": (17, 18), "wght": (400, 700)},
        "scripts": ("Latin", "Cyrillic", "Greek", "Devanagari", "Bengali", "Tamil", "Telugu", "Hebrew", "Thai"),
    },
    "google-sans-code": {
        "name": "Google Sans Code",
        "upright": (
            "ofl/googlesanscode/GoogleSansCode[wght].ttf",
            "c9649573afcd966f61096b1e9c3c3115e7b54315ca2adf9839c6f8b17d5f8e1f",
        ),
        "italic": (
            "ofl/googlesanscode/GoogleSansCode-Italic[wght].ttf",
            "bdf116292f27aca16e2aefb5534042a25f244d3a9187a521ce97fc2117a4a844",
        ),
        "license": ("ofl/googlesanscode/OFL.txt", "ff8f4fbc4f1a71c7d4c6481c48dc2b21c43f8dc15872536667fee45c15d20928"),
        "axes": {"wght": (300, 800)},
        "scripts": ("Latin",),
    },
    "inclusive-sans": {
        "name": "Inclusive Sans",
        "upright": (
            "ofl/inclusivesans/InclusiveSans[wght].ttf",
            "52ae5fdb8eda45633f1e9817ebdb07d8075c89081d51204f16f78394608a5c4e",
        ),
        "italic": (
            "ofl/inclusivesans/InclusiveSans-Italic[wght].ttf",
            "1b94e7bc731a3d91e358bff9f9253ccef36012e214c0ba64661741fa44494b2e",
        ),
        "license": ("ofl/inclusivesans/OFL.txt", "0690e0819628a20c5ee68388a7bae621e5c3e53be57276169decd00111d8f2cf"),
        "axes": {"wght": (300, 700)},
        "scripts": ("Latin",),
    },
    "instrument-serif": {
        "name": "Instrument Serif",
        "upright": (
            "ofl/instrumentserif/InstrumentSerif-Regular.ttf",
            "498efd461f6ddfcb7a111bf9a565709d2085d48201d501ead960d93e84ffbb88",
        ),
        "italic": (
            "ofl/instrumentserif/InstrumentSerif-Italic.ttf",
            "08939b8bdf534afec24ae0ef5e03f948940cd9a8fe08e7fecbad040e62327385",
        ),
        "license": ("ofl/instrumentserif/OFL.txt", "129ed7618959716959f2941fdd5b49e0ad6e6c1d78726761786a00253d865521"),
        "axes": {},
        "scripts": ("Latin",),
    },
    "inter": {
        "name": "Inter",
        "upright": (
            "ofl/inter/Inter[opsz,wght].ttf",
            "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031",
        ),
        "italic": (
            "ofl/inter/Inter-Italic[opsz,wght].ttf",
            "acd98e64795781b2058f07b18475e0ecee2a0fe2b42a49e2f9e37d0d6bf66ce6",
        ),
        "license": ("ofl/inter/OFL.txt", "5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57"),
        "axes": {"opsz": (14, 32), "wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "inter-tight": {
        "name": "Inter Tight",
        "upright": (
            "ofl/intertight/InterTight[wght].ttf",
            "b81b73dcb64df3c230cabade7df6c5773bf863233f24c9ee51087519f1f88b6f",
        ),
        "italic": (
            "ofl/intertight/InterTight-Italic[wght].ttf",
            "15d3edf0c3d2560658529b8c4c7398d06c730a4695484c680b6c9ca9e4385be2",
        ),
        "license": ("ofl/intertight/OFL.txt", "50240ab035cf1b6b3307940235481d515c4b6de3ab1fa843dbe59e7892cb9d58"),
        "axes": {"wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "jetbrains-mono": {
        "name": "JetBrains Mono",
        "upright": (
            "ofl/jetbrainsmono/JetBrainsMono[wght].ttf",
            "48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda",
        ),
        "italic": (
            "ofl/jetbrainsmono/JetBrainsMono-Italic[wght].ttf",
            "85ae2a5cd3f56baf1ce1c21a851322c58e3d8fbe8e8ad4a4d090a820dd7fe558",
        ),
        "license": ("ofl/jetbrainsmono/OFL.txt", "b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d"),
        "axes": {"wght": (100, 800)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "kanit": {
        "name": "Kanit",
        "upright": ("ofl/kanit/Kanit-Medium.ttf", "50a237034244026a7c63fe55e24e0b4298babe363cde043e032075b764c545a2"),
        "italic": (
            "ofl/kanit/Kanit-MediumItalic.ttf",
            "266a8044e599ce2feef7dffa8e3ce7648c94927daba20c4838629054633cf8c4",
        ),
        "license": ("ofl/kanit/OFL.txt", "eb998cc5a685b240cdb46a0b5747e70334c8c57bbd7b1f6b044d75719358df67"),
        "axes": {},
        "scripts": ("Latin", "Thai"),
    },
    "literata": {
        "name": "Literata",
        "upright": (
            "ofl/literata/Literata[opsz,wght].ttf",
            "b41138c9373112f32abb589cc22e8674b06ed4048b0c513be922bdd26f274440",
        ),
        "italic": (
            "ofl/literata/Literata-Italic[opsz,wght].ttf",
            "d483dfaeba9cbf4ce71d32a52ee65df82f7e35b15fff8d1011cdb242d1fcd465",
        ),
        "license": ("ofl/literata/OFL.txt", "8742963604cd89dc81437811a850018fc03b2bfad686d7422c8235967c87614e"),
        "axes": {"opsz": (7, 72), "wght": (200, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "montserrat": {
        "name": "Montserrat",
        "upright": (
            "ofl/montserrat/Montserrat[wght].ttf",
            "0f7b311b2f3279e4eef9b2f968bcdbab6e28f4daeb1f049f4f278a902bcd82f7",
        ),
        "italic": (
            "ofl/montserrat/Montserrat-Italic[wght].ttf",
            "51607f316bc020e59f03cbf51543eecffbea501c0b31d73e5b82927c5cca442c",
        ),
        "license": ("ofl/montserrat/OFL.txt", "8b7141c03fa4f8d44e6345d5d4931709290f0f67875e452e95ac1fd3a027802e"),
        "axes": {"wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic"),
    },
    "noto-sans": {
        "name": "Noto Sans",
        "upright": (
            "ofl/notosans/NotoSans[wdth,wght].ttf",
            "bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d",
        ),
        "italic": (
            "ofl/notosans/NotoSans-Italic[wdth,wght].ttf",
            "58e6e0ebd1931b29a365aa2d3e2ee9a9e831a3af7cf3ad1462d4e72154f0b291",
        ),
        "license": ("ofl/notosans/OFL.txt", "cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a"),
        "axes": {"wdth": (62.5, 100), "wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek", "Devanagari"),
    },
    "noto-sans-jp": {
        "name": "Noto Sans JP",
        "upright": (
            "ofl/notosansjp/NotoSansJP[wght].ttf",
            "c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f",
        ),
        "italic": None,
        "license": ("ofl/notosansjp/OFL.txt", "1c05c68c34f9708415aada51f17e1b0092d2cea709bf4a94cd38114f9e73d7d9"),
        "axes": {"wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic (most)", "Greek", "kana", "Han (most)"),
        "reserved": ('Source',),
    },
    "noto-serif": {
        "name": "Noto Serif",
        "upright": (
            "ofl/notoserif/NotoSerif[wdth,wght].ttf",
            "4d8e6761424656867019081a1a01336f3cb086982682698714054fc33f782713",
        ),
        "italic": (
            "ofl/notoserif/NotoSerif-Italic[wdth,wght].ttf",
            "e87acbc6c0efd0d9a20d6a8cbbda2b266c14be3a3a6f5af8ec9d7b2460570ad1",
        ),
        "license": ("ofl/notoserif/OFL.txt", "cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a"),
        "axes": {"wdth": (62.5, 100), "wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "noto-serif-jp": {
        "name": "Noto Serif JP",
        "upright": (
            "ofl/notoserifjp/NotoSerifJP[wght].ttf",
            "2fd527ba12b6a44ec30d796d633360da0aeba6c5d4af1304ce12bb4dc15a7dfc",
        ),
        "italic": None,
        "license": ("ofl/notoserifjp/OFL.txt", "5e0da210fb04058a8c0087985d2d456b931c2579811a49655721d3cf0c36b6d6"),
        "axes": {"wght": (200, 900)},
        "scripts": ("Latin", "Cyrillic (most)", "Greek", "kana", "Han (most)"),
    },
    "nunito": {
        "name": "Nunito",
        "upright": ("ofl/nunito/Nunito[wght].ttf", "bb55a5ca5c2042335b3991af27c4d0705d0ef41cac6164ac737fd8f2a1e85207"),
        "italic": (
            "ofl/nunito/Nunito-Italic[wght].ttf",
            "b520cc871868b0acfca1beda875df7f4a44ebce914f8a89f83977fc9c09529c8",
        ),
        "license": ("ofl/nunito/OFL.txt", "580df76c95a1ec5ab878ceb25bb3d85c6a076804e9c970c8c6972aea775fdf65"),
        "axes": {"wght": (200, 1000)},
        "scripts": ("Latin", "Cyrillic"),
    },
    "open-sans": {
        "name": "Open Sans",
        "upright": (
            "ofl/opensans/OpenSans[wdth,wght].ttf",
            "36643644f318a812aab2d2ed3bb98f8cf0872527f835fe9398d95fe6b9adb878",
        ),
        "italic": (
            "ofl/opensans/OpenSans-Italic[wdth,wght].ttf",
            "fe269381e992f32e135801740998544d6235061e37c93ec067ad2be3edd5b17b",
        ),
        "license": ("ofl/opensans/OFL.txt", "fbbbcfef55318de350562559b671360de6d597112ecc5c73881b05092db89602"),
        "axes": {"wdth": (75, 100), "wght": (300, 800)},
        "scripts": ("Latin", "Cyrillic", "Greek", "Hebrew"),
    },
    "playfair": {
        "name": "Playfair",
        "upright": (
            "ofl/playfair/Playfair[opsz,wdth,wght].ttf",
            "33cfecbc9948cf9eafe54dc9f36d19a03b393cf12221b20be235926d1ca1db61",
        ),
        "italic": (
            "ofl/playfair/Playfair-Italic[opsz,wdth,wght].ttf",
            "ae0d7a72a93804925040c277b4cd7e0607f48bb68dbbce70e3da6b5949f911ca",
        ),
        "license": ("ofl/playfair/OFL.txt", "7d16bbb17375beefaa4d1d25c41f860cb161c9e614b44af790983a9fd7102627"),
        "axes": {"opsz": (5, 1200), "wdth": (87.5, 112.5), "wght": (300, 900)},
        "scripts": ("Latin", "Cyrillic"),
    },
    "plus-jakarta-sans": {
        "name": "Plus Jakarta Sans",
        "upright": (
            "ofl/plusjakartasans/PlusJakartaSans[wght].ttf",
            "89b3fb38aa0d275d7a731d0d817a4f1622b316b4d7fbdedcf02ee9099ff68bc8",
        ),
        "italic": (
            "ofl/plusjakartasans/PlusJakartaSans-Italic[wght].ttf",
            "9529eb888668b6a3c6dd75b6341a2fc5263fb6c9e788822e6117c29dd9e8b115",
        ),
        "license": ("ofl/plusjakartasans/OFL.txt", "995c7199cab65954f545996326755daee7b63cc6b42b06c13da1f9502ab08a99"),
        "axes": {"wght": (200, 800)},
        "scripts": ("Latin",),
    },
    "poppins": {
        "name": "Poppins",
        "upright": ("ofl/poppins/Poppins-Bold.ttf", "983676516167748b74de6f4771fb384c664fd913acb8b471122ecacf5da5ea6c"),
        "italic": (
            "ofl/poppins/Poppins-BoldItalic.ttf",
            "3572ac8116a0ac7317d342262b29937bcbaf94d8f03f90df6fe666fa7e2fb43a",
        ),
        "license": ("ofl/poppins/OFL.txt", "6be04893d770899a015649c7aa3b582f871b272f8747a92b78b17c3e5c8b2573"),
        "axes": {},
        "scripts": ("Latin", "Devanagari"),
    },
    "pretendard": {
        "name": "Pretendard",
        "source": PRETENDARD,
        "upright": (
            "packages/pretendard/dist/public/variable/PretendardVariable.ttf",
            "3090ccde0442bb347aa7685d9ba8b17436a60682df6e8f92a9a670de14056e22",
        ),
        "italic": None,
        "license": ("LICENSE", "d31ddd9f2bed32fd7e302a205cf2380ba0de6529152d239ef99cfb6f261bfc04"),
        "axes": {"wght": (45, 930)},
        "scripts": ("Latin", "Cyrillic", "Greek", "Hangul", "kana"),
        "reserved": ('Pretendard', 'Source', 'Inter', 'M PLUS 1'),
    },
    "red-hat-display": {
        "name": "Red Hat Display",
        "upright": (
            "ofl/redhatdisplay/RedHatDisplay[wght].ttf",
            "46c9d4c4a2415e7e72020b318f5cda2bcbc9018d78b1a67e480a76d8d6e4b379",
        ),
        "italic": (
            "ofl/redhatdisplay/RedHatDisplay-Italic[wght].ttf",
            "6f1e62869decc48f589b022b88b486b1f3f55792618e79f986a6b656c9378a59",
        ),
        "license": ("ofl/redhatdisplay/OFL.txt", "435fbfb7e66988b2a06686a4cb966faec733f35d8fe100a1601573c27f3e0bb8"),
        "axes": {"wght": (300, 900)},
        "scripts": ("Latin",),
    },
    "roboto": {
        "name": "Roboto",
        "upright": (
            "ofl/roboto/Roboto[wdth,wght].ttf",
            "d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134",
        ),
        "italic": (
            "ofl/roboto/Roboto-Italic[wdth,wght].ttf",
            "9725a847af6b460ffca162ae66d20dad48b01876137947180b42d7dcd7887182",
        ),
        "license": ("ofl/roboto/OFL.txt", "061402327a96aadb0bfb694a960ed289ecd38d383e396243831ab81feb109c41"),
        "axes": {"wdth": (75, 100), "wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "roboto-condensed": {
        "name": "Roboto Condensed",
        "upright": (
            "ofl/robotocondensed/RobotoCondensed[wght].ttf",
            "dace262afcee68a5276f200d8026c57221735c0118ab5fda8c2c0d3dc409a8d0",
        ),
        "italic": (
            "ofl/robotocondensed/RobotoCondensed-Italic[wght].ttf",
            "78f643b1923008b00dfc9b371a2ecd4d80a017722925f1a3fac9940be56d1b7d",
        ),
        "license": ("ofl/robotocondensed/OFL.txt", "0e4cc6ece88573545be2ed25835363662a6182ba4a4c1b5c8feda52add30e8a6"),
        "axes": {"wght": (100, 900)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "roboto-mono": {
        "name": "Roboto Mono",
        "upright": (
            "ofl/robotomono/RobotoMono[wght].ttf",
            "66a80e79d17e4c7cabd162e2916578a4cc08fd19eef6e2a643305eae9c567b2b",
        ),
        "italic": (
            "ofl/robotomono/RobotoMono-Italic[wght].ttf",
            "49ac343bb7b070071e53f0a8a501d68d140ed98ebd0a43b6b8fb96cf22f09ff7",
        ),
        "license": ("ofl/robotomono/OFL.txt", "50ab8dd54680d3473f649c9db86fece88434d097c7834475c1c72d2f8c429215"),
        "axes": {"wght": (100, 700)},
        "scripts": ("Latin", "Cyrillic", "Greek"),
    },
    "rubik": {
        "name": "Rubik",
        "upright": ("ofl/rubik/Rubik[wght].ttf", "1b3a7437ba2af80e465e773ed60c5036d1ba6ace492d89046dbcf18fb31e4e88"),
        "italic": (
            "ofl/rubik/Rubik-Italic[wght].ttf",
            "08c6c4018a5ada8b517407b46897e46cf6ebb106853fbd3e89addb51d3b59c62",
        ),
        "license": ("ofl/rubik/OFL.txt", "472cbe7c25441df63e9c7864b43eb3c0f4b3df950c66a76224e6cfe1eae843fb"),
        "axes": {"wght": (300, 900)},
        "scripts": ("Latin", "Cyrillic", "Arabic", "Hebrew"),
    },
    "shantell-sans": {
        "name": "Shantell Sans",
        "upright": (
            "ofl/shantellsans/ShantellSans[BNCE,INFM,SPAC,wght].ttf",
            "d98a7913853f78d71aa0b86daf66b5eddb5a3266c07bfd51ef70632b88c41084",
        ),
        "italic": (
            "ofl/shantellsans/ShantellSans-Italic[BNCE,INFM,SPAC,wght].ttf",
            "269eb72414b3e7eb66fe5159ce507a61266485c41dbcbc1b44a25fadeb4a2c71",
        ),
        "license": ("ofl/shantellsans/OFL.txt", "ebfce7d104d597c385b71e9fbf8f0e2034e73320f857086105dc265b82c05caa"),
        "axes": {"BNCE": (-100, 100), "INFM": (0, 100), "SPAC": (0, 100), "wght": (300, 800)},
        "scripts": ("Latin", "Cyrillic"),
    },
}
# A cache of upstream files by sha256 that CI keeps, and `--fetch` fills (PLAN T.0b).
CACHE = Path.home() / ".cache/scaena-fonts"
FRAUNCES = (
    "ofl/fraunces/Fraunces[SOFT,WONK,opsz,wght].ttf",
    "177ff6c0f14e5550a3c624247cd1189611d4eb65d000b14944c63d967958abbb",
    "display",
)
INTER = ("ofl/inter/Inter[opsz,wght].ttf", "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031", "body")
JETBRAINS_MONO = (
    "ofl/jetbrainsmono/JetBrainsMono[wght].ttf",
    "48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda",
    "mono",
)
LICENSES = {
    "OFL-Fraunces.txt": ("ofl/fraunces/OFL.txt", "bdf4c22802eaf804f998195871c6b8938aac2ac14b2d78a8bd66a6f1eced833b"),
    "OFL-Inter.txt": ("ofl/inter/OFL.txt", "5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57"),
    "OFL-JetBrainsMono.txt": ("ofl/jetbrainsmono/OFL.txt", "b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d"),
}
# Each family's italic face (PLAN 2.40), which the themes that ship name.
FRAUNCES_ITALIC = (
    "ofl/fraunces/Fraunces-Italic[SOFT,WONK,opsz,wght].ttf",
    "b24448c43702fac4ee856781d461a0dfba8d8e594b6e8e190234b75fed2c0e01",
    "display, italic",
)
INTER_ITALIC = (
    "ofl/inter/Inter-Italic[opsz,wght].ttf",
    "acd98e64795781b2058f07b18475e0ecee2a0fe2b42a49e2f9e37d0d6bf66ce6",
    "body, italic",
)
JETBRAINS_MONO_ITALIC = (
    "ofl/jetbrainsmono/JetBrainsMono-Italic[wght].ttf",
    "85ae2a5cd3f56baf1ce1c21a851322c58e3d8fbe8e8ad4a4d090a820dd7fe558",
    "mono, italic",
)
# Gallery's display family (PLAN 2.102): Instrument Serif, one weight, and its italic. Its body is
# Inter, which draws Greek and Cyrillic where Instrument Serif does not.
INSTRUMENT_SERIF = (
    "ofl/instrumentserif/InstrumentSerif-Regular.ttf",
    "498efd461f6ddfcb7a111bf9a565709d2085d48201d501ead960d93e84ffbb88",
    "display (Gallery)",
)
INSTRUMENT_SERIF_ITALIC = (
    "ofl/instrumentserif/InstrumentSerif-Italic.ttf",
    "08939b8bdf534afec24ae0ef5e03f948940cd9a8fe08e7fecbad040e62327385",
    "display (Gallery), italic",
)
GALLERY_FONTS = {
    "InstrumentSerif-Regular.ttf": INSTRUMENT_SERIF,
    "InstrumentSerif-Italic.ttf": INSTRUMENT_SERIF_ITALIC,
}
GALLERY_LICENSES = {
    "OFL-InstrumentSerif.txt": (
        "ofl/instrumentserif/OFL.txt",
        "129ed7618959716959f2941fdd5b49e0ad6e6c1d78726761786a00253d865521",
    ),
}
# What every subset keeps besides its decks' characters: what `scaena save` keeps of a font
# (scaena-store's `BASE_RANGES`, SPEC §3.1). The torture deck's ranges, the letters Romanian
# writes (ș, ț), and U+2000–27BF: punctuation, super- and subscripts (CO₂), currency, letterlike
# symbols, number forms, arrows, mathematical operators (−, ≈, ≤), technical symbols, enclosed
# alphanumerics, shapes, symbols, and dingbats (PLAN G.21).
BASE_RANGES = [*torture.EXTRA_RANGES, (0x218, 0x21B), (0x2000, 0x27BF)]
# The scripts the shipped families draw beside Latin, which Narrato writes in: Greek and Cyrillic
# (Ukrainian). The themes that ship carry them whole, as a save keeps them where a deck writes in
# them, so a deck can be written and edited in them (PLAN G.21).
SCRIPT_RANGES = [(0x370, 0x3FF), (0x400, 0x4FF)]
ROMANS = {"Fraunces-VF.ttf": FRAUNCES, "Inter-VF.ttf": INTER, "JetBrainsMono-VF.ttf": JETBRAINS_MONO}
# The example decks' themes (Dusk, Daybreak, Ember) set display, body, and mono in these three,
# each with its italic.
EXAMPLE_FONTS = {
    **ROMANS,
    "Fraunces-Italic-VF.ttf": FRAUNCES_ITALIC,
    "Inter-Italic-VF.ttf": INTER_ITALIC,
    "JetBrainsMono-Italic-VF.ttf": JETBRAINS_MONO_ITALIC,
}
# The benchmark decks' theme sets the three romans, and adds Source Serif 4, for quotations, renamed
# Bench Serif: its copyright reserves "Source", which a subset, a Modified Version, may not keep as
# its name (OFL 1.1, condition 3; ADR-0031, call 5; PLAN T.0f).
B1_FONTS = {  # bundle file -> (google/fonts path, upstream sha256, theme family)
    **ROMANS,
    "BenchSerif-VF.ttf": (
        "ofl/sourceserif4/SourceSerif4[opsz,wght].ttf",
        "97b2d4da6e3cb494b5a1e66ae176914d852ccabef49e0c02c0df25f3e39aca0b",
        "text",
    ),
}
B1_LICENSES = {
    **LICENSES,
    "OFL-SourceSerif4.txt": ("ofl/sourceserif4/OFL.txt", "5f94c3fd3a23131a417ab5a0c8452de57e70c3cfb9f604d88241f7065ebf9fd9"),
}
# A subset's names that its copyright reserves, and what it is called instead: each name record
# but the copyright, the trademark, and the license's (KEPT_NAMES) says it so.
RENAMED = {"BenchSerif-VF.ttf": [("Source Serif 4", "Bench Serif"), ("SourceSerif4", "BenchSerif")]}
KEPT_NAMES = {0, 7, 13, 14}
BUNDLES = {
    "b1": {
        "title": "Benchmark B1",
        "bundle": ROOT / "tests/bench/b1.scaena",
        "deck": "deck.json",
        "every_family": True,
        "fonts": B1_FONTS,
        "licenses": B1_LICENSES,
    },
    # B2 (charts) and B3 (shaders) share B1's theme and fonts (scripts/build_bench_decks.py).
    **{
        name: {
            "title": f"Benchmark {name.upper()}",
            "bundle": ROOT / f"tests/bench/{name}.scaena",
            "deck": "deck.json",
            "every_family": True,
            "fonts": B1_FONTS,
            "licenses": B1_LICENSES,
        }
        for name in ["b2", "b3"]
    },
    # The fonts the themes that ship carry (`scaena-ops`' `shipped`, the pages, the Mac): every
    # character a deck in them may be written in, not only the examples'.
    "revenue": {
        "title": "Example decks'",
        "ranges": [*BASE_RANGES, *SCRIPT_RANGES],
        "bundle": ROOT / "docs/examples",
        "deck": "revenue.deck.json",
        # Decks beside it that share its fonts: the subset covers them all, and each is checked.
        "also": ["trails.deck.json", "higher-ed.deck.json"],
        "every_family": False,
        # And Gallery's, which ships beside Dusk, Daybreak, and Ember (PLAN 2.102).
        "fonts": {**EXAMPLE_FONTS, **GALLERY_FONTS},
        "licenses": {**LICENSES, **GALLERY_LICENSES},
    },
    "authorability": {
        "title": "Authorability spike deck",
        "bundle": ROOT / "docs/examples/authorability",
        "deck": "deck.json",
        # Edit (e) moved the deck from Dusk to Daybreak; both must set its text.
        "themes": ["themes/daybreak.theme.json", "themes/dusk.theme.json"],
        "every_family": False,
        "fonts": EXAMPLE_FONTS,
        "licenses": LICENSES,
    },
}


def build(name: str, decks: list[dict], cache: Path | None) -> None:
    import fontTools
    from fontTools import subset
    from fontTools.ttLib import TTFont

    if fontTools.version != torture.FONTTOOLS:
        print(f"warning: fontTools {fontTools.version}, pinned {torture.FONTTOOLS}; subset bytes may differ", file=sys.stderr)
    cfg = BUNDLES[name]
    fonts = cfg["bundle"] / "fonts"
    unicodes = {ord(c) for deck in decks for s in torture.deck_strings(deck) for c in s}
    for lo, hi in cfg.get("ranges", torture.EXTRA_RANGES):
        unicodes.update(range(lo, hi + 1))
    fonts.mkdir(parents=True, exist_ok=True)
    rows = []
    for file, (path, pinned, family) in cfg["fonts"].items():
        data = torture.fetch(path, pinned, cache)
        opts = subset.Options()  # the torture deck's options, minus its emoji special case
        opts.layout_features = ["*"]
        opts.name_IDs = ["*"]
        opts.name_languages = ["*"]
        opts.notdef_outline = True
        opts.hinting = True
        font = TTFont(io.BytesIO(data), recalcTimestamp=False)
        sub = subset.Subsetter(opts)
        sub.populate(unicodes=unicodes)
        sub.subset(font)
        rename(font, RENAMED.get(file, []))
        out = io.BytesIO()
        font.save(out)
        (fonts / file).write_bytes(out.getvalue())
        rows.append((file, path, pinned, torture.sha256(out.getvalue()), len(data), len(out.getvalue()), family))
    for file, (path, pinned) in cfg["licenses"].items():
        (fonts / file).write_bytes(torture.fetch(path, pinned, cache))
    ranges = " ∪ ".join(
        f"U+{lo:04X}–{hi:04X}" if lo != hi else f"U+{lo:04X}" for lo, hi in merged(cfg.get("ranges", torture.EXTRA_RANGES))
    )
    lines = [
        f"# {cfg['title']} fonts: provenance",
        "",
        f"Built by `scripts/build_bundle_fonts.py` (fontTools {torture.FONTTOOLS}) from "
        f"[google/fonts](https://github.com/google/fonts) at commit `{torture.COMMIT}`. Do not edit by hand; rerun the script.",
        "",
        "| Bundle file | Upstream path | Upstream sha256 | Subset sha256 | Upstream → subset | Theme family |",
        "|---|---|---|---|---|---|",
    ]
    for file, path, up, sub_hash, n_up, n_sub, family in rows:
        lines.append(f"| `{file}` | `{path}` | `{up[:16]}…` | `{sub_hash[:16]}…` | {n_up // 1024} KB → {n_sub // 1024} KB | `{family}` |")
    lines += [
        "",
        f"**Subset:** every character in {' ∪ '.join(f'`{d}`' for d in [cfg['deck'], *cfg.get('also', [])])} ∪ {ranges}. "
        "All OpenType layout features, all name records, "
        "variations and hinting kept.",
        "",
        "**Licenses:** SIL Open Font License 1.1; each `OFL-*.txt` beside the fonts is the upstream license file "
        "(sha256-pinned in the script).",
        "",
    ]
    for file, names in RENAMED.items():
        if file in cfg["fonts"]:
            said = " and ".join(f"“{old}” as “{new}”" for old, new in names)
            lines += [
                f"**Renamed:** `{file}` says {said} in every name record but its copyright, trademark, and "
                "license: its copyright reserves the name “Source”, which a subset may not keep (OFL 1.1, condition 3).",
                "",
            ]
    (fonts / "SOURCES.md").write_text("\n".join(lines), encoding="utf-8")
    print(f"built {len(rows)} fonts into {fonts.relative_to(ROOT)}")


def rename(font, names: list[tuple[str, str]]) -> None:
    """`font`'s names, each `(old, new)` replaced in every name record but those KEPT_NAMES holds."""
    for record in font["name"].names:
        if record.nameID in KEPT_NAMES:
            continue
        text = record.toUnicode()
        for old, new in names:
            text = text.replace(old, new)
        record.string = text


def reserved(path: Path) -> bool:
    """Whether the font at `path` is named by a name its copyright reserves (OFL 1.1, condition 3):
    a subset is a Modified Version, which may not be. The copyright says "with Reserved Font Name
    'X'" (or Names, each quoted); a name record outside KEPT_NAMES that says X uses it."""
    import re

    from fontTools.ttLib import TTFont

    names = TTFont(path, lazy=True)["name"]
    copyright = names.getDebugName(0) or ""
    found = re.search(r"Reserved Font Names?\s+(.*)", copyright, re.I)
    words = re.findall(r"[‘'\"“]([^’'\"”]+)[’'\"”]", found[1]) if found else []
    used = {
        (record.nameID, word)
        for record in names.names
        if record.nameID not in KEPT_NAMES
        for word in words
        if word.casefold() in record.toUnicode().casefold()
    }
    for nid, word in sorted(used):
        print(f"error: {path.relative_to(ROOT)}: name {nid} uses `{word}`, a Reserved Font Name")
    return bool(used)


def pinned(source: tuple[str, str], path: str, expected: str, cache: Path, offline: bool, get=None) -> bytes:
    """The file at `path` in `source`'s repository at its ref, held to its sha256: from `cache`
    where it holds them, else downloaded into it (never `offline`)."""
    import urllib.parse
    import urllib.request

    cached = cache / expected
    if cached.exists() and torture.sha256(cached.read_bytes()) == expected:
        return cached.read_bytes()
    if offline:
        raise SystemExit(f"{path}: not in {cache}, and --offline downloads nothing")
    repo, ref = source
    url = f"https://raw.githubusercontent.com/{repo}/{ref}/{urllib.parse.quote(path)}"
    get = get or (lambda url: urllib.request.urlopen(url, timeout=300).read())
    for attempt in range(5):  # large files are sometimes cut off in transit; the hash catches the rest
        try:
            data = get(url)
            break
        except OSError as e:
            if attempt == 4:
                raise SystemExit(f"download failed for {path}: {e}") from e
    if torture.sha256(data) != expected:
        raise SystemExit(f"sha256 mismatch for {path}: got {torture.sha256(data)}, pinned {expected}")
    cache.mkdir(parents=True, exist_ok=True)
    cached.write_bytes(data)
    return data


def family_files(family: dict) -> list[tuple[str, str]]:
    """A family's files, `(path, sha256)`: its upright, its italic if it has one, and its license."""
    return [f for f in (family["upright"], family["italic"], family["license"]) if f]


def fetch_families(cache: Path, offline: bool) -> None:
    """Every family's files into `cache`, each held to its sha256."""
    n = 0
    for family in FAMILIES.values():
        for path, expected in family_files(family):
            pinned(family.get("source", GOOGLE_FONTS), path, expected, cache, offline)
            n += 1
    print(f"{n} files of {len(FAMILIES)} families in {cache}{' (offline)' if offline else ''}")


def pins_agree() -> bool:
    """Whether every upstream file this script pins elsewhere is pinned as FAMILIES pins it."""
    pins = {path: sha for family in FAMILIES.values() for path, sha in family_files(family)}
    ok = True
    for cfg in BUNDLES.values():
        for path, sha, *_ in [*cfg["fonts"].values(), *cfg["licenses"].values()]:
            if path in pins and pins[path] != sha:
                ok = False
                print(f"error: {path} is pinned {sha[:16]}… for a bundle and {pins[path][:16]}… in FAMILIES")
    return ok


# The files a provenance table answers for: fonts, pictures, and data beside it.
HELD = {".ttf", ".otf", ".woff2", ".png", ".jpg", ".jpeg", ".csv", ".json"}


def sources_held(sources: Path) -> bool:
    """Whether the `SOURCES.md` at `sources` answers for the files beside it (PLAN T.0b): each
    file a row of its tables names is there, its sha256 what the row's own sha256 column says
    (`sha256`, or a subset's), and each font or picture beside it has a row."""
    import re

    ok, named, where = True, set(), sources.relative_to(ROOT) if sources.is_relative_to(ROOT) else sources
    column = None
    for line in sources.read_text(encoding="utf-8").splitlines():
        if not line.startswith("|"):
            column = None
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if column is None:  # a table's header
            column = next((i for i, c in enumerate(cells) if c in ("sha256", "Subset sha256")), -1)
            continue
        files = re.findall(r"`([^`/]+\.[A-Za-z0-9]+)`", cells[0])
        if not files or set(cells[0]) <= set("-: "):
            continue
        said = cells[column].strip("`… ") if 0 <= column < len(cells) else ""
        for file in files:
            named.add(file)
            path = sources.parent / file
            if not path.exists():
                ok = False
                print(f"error: {where}: a row names `{file}`, which is not beside it")
            elif said and not torture.sha256(path.read_bytes()).startswith(said):
                ok = False
                print(f"error: {where}: `{file}` is {torture.sha256(path.read_bytes())[:16]}…, and its row says {said}")
    for path in sorted(sources.parent.iterdir()):
        if path.suffix.lower() in HELD - {".json", ".csv"} and path.name not in named:
            ok = False
            print(f"error: {where}: `{path.name}` is beside it with no row")
    return ok


def every_sources() -> list[Path]:
    """Every `SOURCES.md` in the repository's docs and tests."""
    return sorted(p for top in ("docs", "tests") for p in (ROOT / top).rglob("SOURCES.md"))


def merged(ranges: list[tuple[int, int]]) -> list[tuple[int, int]]:
    """`ranges` in order, those that overlap or touch made one."""
    out: list[tuple[int, int]] = []
    for lo, hi in sorted(ranges):
        if out and lo <= out[-1][1] + 1:
            out[-1] = (out[-1][0], max(out[-1][1], hi))
        else:
            out.append((lo, hi))
    return out


def check(name: str, deck: dict, theme: dict, label: str) -> bool:
    from fontTools.ttLib import TTFont

    cfg = BUNDLES[name]
    bundle = cfg["bundle"]
    families = theme["type"]["families"]
    roles = theme["type"]["roles"]
    cmaps = {key: TTFont(bundle / fam["file"], lazy=True).getBestCmap() for key, fam in families.items()}
    texts = []  # (node id, text, role)
    for nid, node in deck["nodes"].items():
        if node["type"] != "text":
            continue
        role = node.get("role", "body")
        texts += [(nid, text, run_role or role) for text, run_role in strings(node)]
        for state in deck["states"]:
            delta = state.get("props", {}).get(nid, {})
            role = delta.get("role", role)
            texts += [(nid, text, run_role or role) for text, run_role in strings(delta)]
    ok, used = True, set()
    for nid, text, role in texts:
        family = roles[role]["family"]
        used.add(family)
        for ch in text:
            if ch == "\n" or ord(ch) in torture.IGNORABLE:
                continue
            if ord(ch) not in cmaps[family]:
                ok = False
                print(f"error: {nid}: U+{ord(ch):04X} {unicodedata.name(ch, '?')} is not in `{family}` (role `{role}`)")
    if cfg["every_family"] and used != set(families):
        ok = False
        print(f"error: the deck sets text in {sorted(used)}, but its theme has {sorted(families)}")
    print(f"{label} coverage: {len(texts)} strings in {len(used)} families {'ok' if ok else 'FAILED'}")
    return ok


def strings(props: dict):
    """A text node's (or delta's) strings, with the role a run sets, if any."""
    if "text" in props:
        yield props["text"], None
    for run in props.get("runs", []):
        yield run["text"], run.get("role")


def held_to_the_save() -> bool:
    """Whether the ranges here are the ones a save keeps (scaena-store's `BASE_RANGES`, and
    among its `SCRIPT_RANGES`): a shipped font without them would lose glyphs a save expects to
    keep, and a save that kept fewer would cut what the shipped fonts carry for editing."""
    import re

    rust = (ROOT / "crates/scaena-store/src/subset.rs").read_text(encoding="utf-8")

    def ranges(name: str) -> list[tuple[int, int]]:
        found = re.search(rf"pub const {name}: \[\(u32, u32\); \d+\] =\s*\[(.*?)\];", rust, re.S)
        pairs = re.findall(r"\(0x([0-9A-Fa-f]+), 0x([0-9A-Fa-f]+)\)", found[1]) if found else []
        return [(int(lo, 16), int(hi, 16)) for lo, hi in pairs]

    ok = True
    if merged(ranges("BASE_RANGES")) != merged(BASE_RANGES):
        ok = False
        print(f"error: scaena-store's BASE_RANGES is {ranges('BASE_RANGES')}, and this script's {merged(BASE_RANGES)}")
    saved = ranges("SCRIPT_RANGES")
    for script in SCRIPT_RANGES:
        if script not in saved:
            ok = False
            print(f"error: scaena-store's SCRIPT_RANGES {saved} keep no {script}, which the shipped fonts carry")
    return ok


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="check coverage only; no network, no writes")
    ap.add_argument("--cache", type=Path, help="directory of upstream files keyed by sha256 (read, then filled)")
    ap.add_argument("--only", action="append", choices=list(BUNDLES), help="build only this bundle (repeatable); all are checked")
    ap.add_argument("--fetch", action="store_true", help="fetch every family a theme names into the cache, then stop")
    ap.add_argument("--offline", action="store_true", help="with --fetch: download nothing, and fail on a file the cache lacks")
    args = ap.parse_args()
    if args.fetch:
        fetch_families(args.cache or CACHE, args.offline)
        return
    ok = held_to_the_save() & pins_agree()
    for sources in every_sources():
        ok &= sources_held(sources)
    for name, cfg in BUNDLES.items():
        files = [cfg["deck"], *cfg.get("also", [])]
        decks = [json.loads((cfg["bundle"] / f).read_text(encoding="utf-8")) for f in files]
        if not args.check and (not args.only or name in args.only):
            build(name, decks, args.cache)
        for font in sorted((cfg["bundle"] / "fonts").glob("*.tt[fc]")):
            ok &= not reserved(font)
        for file, deck in zip(files, decks):
            label = name if file == cfg["deck"] else file.removesuffix(".deck.json")
            for theme_path in cfg.get("themes", [deck["theme"]]):
                theme = json.loads((cfg["bundle"] / theme_path).read_text(encoding="utf-8"))
                ok &= check(name, deck, theme, f"{label} ({theme_path})")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
