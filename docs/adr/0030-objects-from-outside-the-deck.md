# ADR-0030: Objects from outside the deck: kept in the bundle, refreshed by an edit, credited, and drawn in the theme

**Status:** proposed · **Date:** 2026-10-15

## Context

Jay asked for four objects on 2026-10-15 (PLAN, "Objects from outside the deck", O.1–O.4):
- **A map:** "give an address and get back a map object."
- **A forecast:** "provide a city or zip and get back a day or week weather forecast."
- **A price chart:** a ticker symbol in, and a chart of a day, a week, a month, a quarter, a year, or the year to date out.
- **A haiku:** "provide either a topic or just point at the deck and ai writes a nice haiku and formats it beautifully."

Three come from a service outside the deck, and the fourth from the user's own model. A research pass read each candidate service's own pages on 2026-10-15, and a second reader opened them again the same day. They are listed under Sources, and a claim that rests on a page that could not be opened says so.

**What this design stands on**, in the repository at dc9f7b5:
- **Data sources** (SPEC §3.10, `scaena_core::data`). Rows are inline or a CSV or JSON file under `data/`, typed by `schema`, and handed to the engine as bytes. `data_attach` declares one (`scaena_ops::create::attaching`) and `data_edit` edits one. The history keeps a data file's bytes with each edit (ADR-0014), and a run's `quote` sets a figure from its data again at every write (ADR-0019).
- **Images** (SPEC §3.3). A PNG or a JPEG is decoded in Rust and kept at `assets/<sha256>.<ext>`, at most 8,192 px a side. `crop`, `fit`, and `focal` frame it in its box, and the editors frame it by handles (PLAN 2.74). Images are named by their content and are not among the history's files (SPEC §8.1).
- **Make a picture** (PLAN 2.107) already fetches a picture with the user's key and makes it an image in one patch. The browser plans the request in the generator's module (`pictureAsked`); the Mac and the iPad take the C ABI's `picture` and `pictureMade` steps. The picture lands in the image selected, else a slide's empty place for a picture (`Waiting::picture`), else where the pointer last pressed.
- **Charts.** There are nine kinds. A line chart runs on a time axis that ticks by the calendar (PLAN 2.81, G.18), lit in the signal color with its claim marked (G.19).
- **Icons.** Phosphor 2.1.1 (MIT), regular and fill, is in `docs/icons/phosphor.json`, `map-pin` and the sky's icons among them (PLAN 2.104). The web build carries the set beside its pages, fetched the first time the picker opens or a made deck names an icon, so the editor's module carries none of it (G.23).
- **Requests built in Rust, made by the client.** This is how the assistant's conversation (`scaena-chat`, ADR-0022), Narrato (ADR-0026), and grounding's fetches (`scaena_sources::fetch`, PLAN G.4) work: a plan gives a GET, the client makes it, and hands the answer back. A browser's plan sets no User-Agent. The writing tools' words are the engine's (`scaena_core::writing`, PLAN 2.101).
- **Lint** (SPEC §7.4, §7.5). A finding's `fix` is a patch, and no rule reads a clock. The codes taken or planned are E100–E106, E110, E111, E120, W200–W205, W210, W220, W221, W230–W232, W300–W303, W310–W313, W320–W323, W401, W410, W420–W427, and I400–I403.
- **The layout library** (`scripts/build_layouts.py`, theme formats 0.11–0.13). A slot has a `prompt`, an `example` (a picture, a chart, or a table), and a `fit`. There are 128 layouts in 13 sections.
- **Text roles.** The shipped themes set display, headline, title, lede, kicker, body, caption, label, figure, quote, numeral, hero, code, axis, and value. None sets verse.

**What must hold** (CLAUDE.md's invariants):
- `render(document, theme, state, t, viewport)` is pure, with no fetch, clock, or randomness.
- Nothing render-derived enters the CRDT.
- There is no server of Scaena's. A key is the user's own and never goes in a bundle.
- A deck names roles, tokens, and presets, not literals.
- Every operation lives in `scaena-ops` and returns a typed result; the CLI and the MCP server only parse and print.
- The editor's module stays under SPEC §15's 3,000,000 bytes gzipped. It had 150,212 bytes of room when PLAN 2.130 measured it on 2026-10-14, before code's highlighter (2.132) spends an estimated 40–80 KB.

**Formats.** The deck format is 0.27 and the theme format 0.17. Code on a slide takes deck formats 0.28–0.30 and theme formats 0.18–0.20 (PLAN 2.131–2.140).

## Decided (Jay's call, 2026-10-15)

In Jay's words, "maps - provider": **a map is a provider's picture.**
- It is an image fetched once, with the user's own key, from a map service, and kept in the bundle as an image node's file.
- It is credited on the slide as the service's terms ask.
- Its pin and the pin's label are drawn by the engine over the picture, in the theme's roles, not baked into it.

The engine-drawn vector map was considered and not chosen. OpenStreetMap's features, kept in the bundle from free vector tiles and drawn in the theme's colors, would have re-themed with the deck and needed no key. But it was a plainer map, without street names at first, five tasks before it shipped, tile decoding and drawing among them, and an estimated 15–30 KB of the editor's module. A provider's picture gives full cartography for a fraction of the work.

The PLAN's rule that each object is drawn in the theme now holds for what Scaena draws over a map's picture.

## Decision

### What the four share

1. **Fetched once into the bundle; render never fetches.** What a map, a forecast, or a price chart shows is fetched when it is made, by an editor, the CLI, or an agent, and kept in the bundle:
   - a map's picture as an image's file (`assets/<sha256>.png`);
   - a forecast's rows inline in its data source;
   - a price series as a CSV under `data/`.

   Render reads the bundle alone, so the deck draws the same offline, in a single file (which never fetches), in the PDF, and in video, on every painter. What is stored is what was fetched and where it came from, never what was drawn.
2. **One record says where it came from.** An image and a data source take `fetched` (deck format 0.31 for an image, 0.32 for a data source):

   ```jsonc
   "fetched": {
     "from": "geoapify",                        // the service, by Scaena's name for it
     "at": "2026-10-15T20:12:44Z",              // when its answer came, UTC, by the clock of the client that asked
     "asked": "1600 Pennsylvania Ave, Washington",  // what the person typed
     "credit": [                                // what the service's terms ask shown, as runs
       { "text": "© OpenStreetMap contributors", "link": "https://www.openstreetmap.org/copyright" },
       { "text": " · openstreetmap.org/copyright · " },
       { "text": "© OpenMapTiles", "link": "https://openmaptiles.org/" },
       { "text": " · " },
       { "text": "Powered by Geoapify", "link": "https://www.geoapify.com/" }
     ],
     "license": "ODbL-1.0",                     // the data's license, where the service names one
     "map": { … }                               // what was asked, in the object's terms: `map`, `forecast`, or `stock`
   }
   ```

   - **Optional fields.** `terms` says which of a service's terms applied, where the request itself says so: Open-Meteo's free service or its `customer-` one. `kept` says the person keeps a forecast or a series as of its date. `edited` says its rows were edited since they came, as CC BY asks a change to be indicated.
   - **No secrets.** No key, account, or URL that holds a key has a field.
   - **`scaena_core::fetched`** holds the record and a table of the services by name, saying what each asks to be credited.
   - **Validation.** An unknown service, an `at` that is no instant, or an empty credit where the service asks for one is E106. A kind's fields that its data contradicts are E103.
   - **Files.** `scaena files` and the Files tab name each fetched file's service and date.
   - **Fonts.** The credit and the place's name are strings of the deck, so a save's subset keeps their characters (SPEC §3.1).
3. **A refresh is an edit.** Refresh (Fetch Again, for a map) runs the plan again for what the record asked, and writes the new picture or rows and the new record in one patch:
   - by its author: `user`, or `agent:` and the model's name;
   - one step to undo, recorded in the history: a data file with its bytes (ADR-0014), a picture as the change of `src`. Both pictures stay in the bundle, until the Files tab or `scaena files --remove` takes out the one nothing names.

   A redo does not fetch again. A refresh that fails changes nothing and says why, in the service's own words where it gives them.
4. **Each request is built in Rust and made by the client,** as grounding's fetches are (PLAN G.4). A crate of its own, `scaena-objects`, holds the plans.
   - **A plan.** `Plan::new` gives the first `Get`: a URL, its headers, the most bytes its answer may hold, and the slot the key fills, a header or a query parameter. `answer(status, content type, body)` gives the next `Get`, the places to choose from when several match, or what was fetched, typed (`scaena_core::fetched`, `::weather`, and the price series).
   - **What it never does.** It makes no HTTP call, reads no clock, and holds no key. The client puts the key in its slot as it sends, so no request, error, or log that Rust writes holds one. A browser's plan sets no User-Agent.
   - **Why a slot.** This departs from `scaena-chat` and grounding's GitHub plan, which take the key and write it into the headers they build. Geoapify's key rides in the URL's query, and the CLI's errors name the URL they failed on (`crates/scaena-cli/src/sources.rs`), so a key written by Rust would be printed with them.
   - **The loop.** Handing each answer back for the next `Get` is `scaena-objects`' own loop, run over a transport the client gives. `scaena-objects` depends on `scaena-core` alone. The generator's module, the C ABI, the CLI, and the MCP server depend on it, so CLAUDE.md's rule of downward dependencies gains `cli`/`mcp`/`generator`/`ffi → objects → core`.
   - **Who makes each GET.**
     - The CLI, over `ureq`, as `scaena generate` does: the system's certificates, the environment's proxy, and the key from the environment.
     - The MCP server, through the CLI's transport handed to it, so `scaena-mcp` links no HTTP client. Each tool's description says what it sends where.
     - The browser's editor worker, by `fetch`. It runs the plan from the generator's module, loaded the first time (PLAN G.5), so the editor's module carries none of it.
     - The Mac and the iPad, by `URLSession`, through the generator's C ABI steps (`map` and `mapAnswered`, `forecast` and `forecastAnswered`, `stock` and `stockAnswered`), as Make a picture's `picture` and `pictureMade` are taken.
   - **The patch.** What a plan fetched goes to an operation in `scaena-ops` that makes the patch and returns a typed result (`scaena_ops::maps`, `::weather`, `::stock`), which every client calls (ADR-0009). The assistant's tools are the MCP tools, run by its client as that client's Insert runs them.
5. **The user's own key, kept where the assistant's are.** That is the browser's tab, or its device encrypted (`web/src/assistant/keys.ts`), and the Keychain on the Mac and the iPad, an item per service kept in Settings. The CLI reads `GEOAPIFY_API_KEY`, `OPEN_METEO_API_KEY`, and `TWELVEDATA_API_KEY`. A key never goes in a bundle. There is no key of Scaena's to share, which the terms of Open-Meteo's subscriptions and Stadia's would forbid in any case.
6. **The credit is drawn where a license asks.** The engine draws the record's credit with the object: a map's in a corner of its picture or under it, and a chart's or a table's at its foot.
   - **How it is set.** In the theme's `caption` role, never under 12 pt at presentation size. Each run is linked (`link` ops, PLAN 2.70), and the URLs a license asks to be printed are written out.
   - **Who reads it.** It reads after the object: in the player's live region, a single file, and VoiceOver.
   - **Exports.** The PDF carries it as text with link annotations, tagged as a paragraph after the figure, not as an artifact. Video shows it whenever the object shows.
   - **Seen.** No theme or deck can hide it, and W314 (new) says where it cannot be read.
7. **The theme draws what Scaena draws** (ADR-0005). The pin, its label, the credit, the forecast's icons and figures, and the price line are in the theme's roles, colors, and icons, so a re-theme or a theme from a photo restyles them. A map's picture is the provider's (Decided): Scaena picks its style, light or dark, by the theme's surface, and draws nothing into it.
8. **Dates are lint's, handed in.** A forecast passes its days and prices go stale, so lint takes `now` from whoever runs it:
   - the editors, from their clock at each lint;
   - `scaena lint --now`, else `SOURCE_DATE_EPOCH`, else the clock;
   - `deck_lint`'s `now`.

   W428 (new) says when a fetched object has passed its date. A fetch is no patch, so the finding carries no `fix`: an optional `refresh` names what to fetch again (SPEC §7.4). Without `now`, no date is judged, so render and validation stay as they are. None of it enters the CRDT.

   The CLI's `lint` falls back to the clock, and `crates/scaena-cli/tests/lint.rs` lints the torture deck to a golden and the example decks clean. So every fetched source a test or a golden reads is `kept`: the torture deck's forecast, `docs/examples/weather.deck.json`, and the stock example. W428's own fixtures pin `now`.
9. **What each costs the browser's editor module.** The plans live in the generator's module, which the editor loads the first time an object is asked for. What draws an object, checks it, and makes its patch lives in the editor's module, and the player's module draws it too. The estimates:

   | Object | In the editor's module, gzipped (estimated) | Handed over where the room runs short |
   |---|---|---|
   | Shared: the record, the credit, W314, W428, lint's `now` | 5–10 KB | nothing |
   | Map: the pin, its label, `map-pin`'s path | under 5 KB | nothing |
   | Forecast: the weather chart, the WMO table, 11 icons (4,273 bytes of paths, gzipped) | 15–25 KB | the icons' paths (ADR-0015) |
   | Price chart: the `stock` record, its choices, `stocking` | 10–15 KB | `stocking`'s builder, to the generator's module |
   | Haiku: the counter and its table (7,758 bytes, gzipped), the prompts | about 12 KB | the word table (ADR-0015) |
   | All four | 45–70 KB | |

   The room left would be about 65 KB at best, and none at worst, once 2.132's highlighter is spent. So each object's PR measures both modules first, as 2.130 did. Where the room would fall under 100 KB, it hands its table over from the start: the engine gets it the first time it is needed, held to its SHA-256, as the hyphenation patterns are (ADR-0015), and the player's module compiles it in. If the room runs out even so, the budget is revisited in an ADR, not silently relaxed, as SPEC §15 says of video.

### O.1 The map: a provider's picture with the engine's pin

10. **The recommendation: Geoapify first, on every client.** Stadia Maps can follow as a second service behind the same plan (PLAN O.1f).
    - **Keeping, printing, and the PDF.** Geoapify's static maps page says its maps may be "freely" cached, stored, and redistributed, and that their print version may be used in commercial products at no extra cost, with OpenStreetMap's credit. Its terms (version 5, 2 February 2024) ask for OpenStreetMap's credit always and Geoapify's on the Free plan, and say nothing against keeping a map.
    - **Free tier.** 3,000 credits a day, up to 5 requests a second. Its pricing page allows commercial use on the Free plan, in production, with the credits. Its terms say that use in production comes "with some limitations" and to contact it for details, so Geoapify is asked, before the editors ship, whether a presentation app whose users each bring their own Free key fits.
    - **Cost of a map.** One credit, plus a credit for each four 256-px tiles of the picture. By the area alone that is about 9 credits at 1,920 × 1,080, and about 33 if its scale factor's pixels are counted, which its documentation does not say. Its own example counts up to nearly twice the tiles the area gives (15–20 for 1,200 × 600, where the area gives 11), so a full-slide map may cost about 15 credits, or about 60 with its scale factor's pixels. Even 60 credits a map leaves the Free plan 50 maps a day.
    - **CORS.** Its static map and geocoder answer any origin: `Access-Control-Allow-Origin: *`, observed on unauthorized answers.
    - **Its own credit.** By default it writes the map style's credit into the picture's bottom-right corner, on every plan. A paid plan may have that hidden on request. Scaena draws the credit as well (decision 14).
    - **Its geocoder.** It takes the same key at one credit a request. Its results may be stored ("we do not restrict you from storing results") with their sources credited. Each result names its source and that source's credit (`datasource`), which the plan adds to the record's credit where it is not OpenStreetMap's.

    Stadia Maps fits a user who pays for it:
    - Static maps need its Starter plan, $20 a month. Its cacheable endpoint costs 2,000 credits a map, so Starter's 1,000,000 credits are 500 maps a month, with more at 3¢ per 1,000 credits.
    - Under its terms in force (2026-03-18), a cacheable picture may be kept and embedded in documents while a paid subscription is kept, and printed up to 5,000 copies.
    - From 2026-11-09, "Digital Embeddable Usage" lets a picture be embedded for good and redistributed up to 5,000 copies. It covers formats where "the original image can not be served", and a bundle carries the picture as a file of its own. So a deck may be its "Digital Cacheable Usage" instead, which lasts only while a paid subscription does. Stadia is asked which, before O.1f.
    - Its geocoding results may be stored permanently only while the account holds its Standard plan ($80) or above.
    - Its CORS preflight allows `Stadia-Auth`, not `Authorization`, so a browser sends its key in the query.

    Two services are not offered:
    - **MapTiler.** Its Cloud terms forbid storing or redistributing map content "by using a screenshot or other static image", and allow only prints "for an internal purpose", up to A4.
    - **Mapbox.** Its terms of service (31 March 2024) ask that anything downloaded from it be destroyed when the licence ends, so a deck could not keep its map past the user's account. Its static images are at most 1,280 px a side, and its unauthorized answer sends no CORS header.

    **Apple's MapKit snapshots are not used on the Mac and the iPad.** The Apple Developer Program License Agreement defines the Apple Maps Service to include "related tools for capturing map content (e.g., MapSnapshotter)", and its Attachment 6 holds that service's Map Data, imagery among it:
    - §2.5 lets Map Data be cached, pre-fetched, or stored only "on a temporary and limited basis", then deleted, unless Apple permits otherwise in writing or in MapKit's documentation.
    - §2.3 forbids copying, publishing, or publicly displaying Map Data except as it expressly permits.
    - MKMapSnapshotter's documentation permits nothing more.

    A deck keeps its map, shows it to a room, and exports it, so a snapshot cannot be its picture. One service serves every client.
11. **The request** (`scaena_objects::map`):
    - **The place.** Geoapify's `geocode/search` (`text`, `lang` the deck's language as its two letters, `limit=5`, `format=json`), skipped for coordinates typed as `lat, lon`. Several places found are offered to pick from.
    - **The picture.** `maps.geoapify.com/v1/staticmap` with `style`, `width`, `height`, `scaleFactor=2`, `center=lonlat:…`, `zoom`, and `format=png`, the key in `apiKey`. No marker is asked for.
    - **The style.** `positron` on a light surface, `dark-matter` on a dark one, or `osm-bright` where asked.
    - **The zoom.** Show sets it from what the box spans: a block (about 500 m across), a neighborhood (2 km), a city (20 km), or a region (200 km). It is computed with `libm`, so every client asks for the same picture.
12. **Size and scale.**
    - **What is asked.** The picture is the box's size in canvas units, at twice the density (`scaleFactor=2`): a full-slide map is 3,840 × 2,160 px.
    - **On screen and in print.** The PDF draws the canvas at 2 units to the point (SPEC §10), so that map spans 960 pt (13.3 in) at 288 pixels to the inch, print quality. On a 4K display it is a pixel for a pixel.
    - **PNG**, which keeps roads and words sharp where a JPEG blurs them.
    - **Within the limits** of Geoapify's 4,096 px a side and the bundle's 8,192.
    - **Another shape.** A box resized past its picture, or a format's box of another shape (ADR-0020), crops the picture by `fit: cover`, and Fetch Again fetches one for the box. Framing by `crop` and `focal` pans inside the picture with no fetch (PLAN 2.74).
13. **The pin and its label are the engine's.** An image takes `pin` (deck format 0.31): `{ "at": [x, y], "label"?, "color"? }`.
    - **Its fields.** `at` is in fractions of the picture, as `crop` is, set by the plan where the place falls (the middle, when the picture is centered on it, as Insert's is). `label` is the place's name as the geocoder gives it, or words written in, and `""` gives none. `color` is a theme color, the accent by default.
    - **How it is drawn.** Phosphor's `map-pin` in its fill weight, compiled in as one path, its tip on the point through the image's crop, fit, and focal. It is 48 cu tall at 1080p, with a ring of the surface color so it reads at 3:1 on any map.
    - **The label.** In the theme's `label` role, on a card of the surface color beside the pin where it fits inside what shows, and never over the credit.
    - **Out of view.** A pin outside what shows is not drawn.
    - **Moved.** A drag moves the pin, a `choose` of `pin/at`. Any image may take a pin.
    - **Another view.** A pin in a picture centered elsewhere is placed by Web Mercator at the service's tile size, held to the service's own marker by a recorded answer.
14. **The credit.**
    - **What it says.** Geoapify asks for OpenStreetMap's credit, OpenMapTiles' (its styles are built on that schema), and, on the Free plan, its own. OSMF's guideline asks that the URL openstreetmap.org/copyright be printed in a PDF or in print, and a deck is both. So the plan writes the credit shown in decision 2, each part linked.
    - **Where it goes.** On a card in the picture's bottom-left corner, clear of the credit Geoapify writes in the bottom right (`credit: "inside"`, the default), or as a line under the picture inside the node's box (`credit: "below"`).
    - **Its length.** The whole credit runs to 97 characters, about 1,150 cu in a 24-cu caption. It takes one line on a full-slide map, two on a half-slide map, and three on a picture of a four-up slide. Where it would cover more than a quarter of what shows, W314 says so and offers `credit: below`. Both OSMF's guideline and Geoapify's documentation allow a printed credit in a publication's acknowledgments instead. A deck's credits slide would be that place, which this design leaves for later.
    - **Geoapify's own.** The credit Geoapify writes into the picture stays where it is. A crop or a box of another shape may cut it, which is why Scaena draws its own.
15. **Refresh** is Fetch Again. It asks for the same place and view, or the view the inspector asks: Zoom In, Zoom Out, Center on the Pin, a box resized, another style. A new picture, `src`, `pin.at`, and the record go in one patch, one step to undo.
16. **The editors.** Insert › Map… sits in the browser's Insert menu and palette, on the Mac and the iPad in the Insert menu, and on the canvas's right click and long press.
    - **The sheet.** Where (an address, a place, or `lat, lon`, searched on Return, never as it is typed), Show, and Label. Up to five places are offered to pick from. A line says that the words typed and the map's area go to Geoapify, under its terms.
    - **Where it lands.** As Make a picture lands: in the image selected, else in the slide's first empty place for a picture, else where the pointer last pressed. It stays on its slide, in one patch.
    - **The Format tab.** It shows "Map from Geoapify, fetched 15 October 2026", then Place, Label, Pin, Style, Credit, Zoom In, Zoom Out, Center on the Pin, and Fetch Again.
    - **The key.** Settings keeps it. Without one, the sheet says where one goes.
    - **Offline,** it says so and changes nothing.
    - **For agents.** `scaena map` and the MCP tool `map_fetch`.
17. **Lint.** W314 for its credit. There is no date rule: a map does not pass its date in a deck's life, and the inspector says when it was fetched. A missing picture is E102, as for any image.

### O.2 The forecast: a data source drawn as a weather chart

18. **The source.** A forecast is a data source whose rows are written inline, so a refresh is one patch and every export carries it.
    - **By the day:** `date`, `sky` (the WMO code), `high`, `low`, `rain` (the chance, in percent), `amount`, `wind`, and `direction`.
    - **By the hour:** `time`, `sky`, `temperature`, `rain`, `amount`, `wind`, `direction`, and `day` (daylight).
    - **`fetched.forecast`:** the place (name, region, country, latitude, longitude, time zone, and the UTC offset the answer's days were cut by), the period (`day` or `hour`), and the units as fetched.
    - **`scaena_core::weather`** holds the WMO table: each code Open-Meteo documents, to a sky, its icon by day and by night, and its words in the six locales `docs/spec/format.md` lists.
19. **The weather chart**, a tenth chart kind, `weather` (deck format 0.33).
    - **Its columns.** Each period is a column across the box, or a row down it where the box is taller than wide (a 9:16 format), and one period alone is drawn large.
    - **What a column holds:** the period's name (the weekday, or the hour as the time axis prints hours), the sky's icon, the high, the low in the muted color, and the chance of rain with a drop where it is 10% or more. The sky's words and the wind are parts a person can add.
    - **The icons** are Phosphor's regular `sun`, `cloud-sun`, `cloud`, `cloud-fog`, `cloud-rain`, `cloud-snow`, `cloud-lightning`, `moon`, `cloud-moon`, `drop`, and `wind`, compiled in by `scripts/build_icons.py`. An hour of night takes the moon.
    - **Units belong to the view.** The chart converts from the source's units at layout, in IEEE arithmetic the same on every target, so a change of units fetches nothing.
    - **The rest.** A `highlight` picks a period. The parts take the chart's roles until a theme names its own (`charts.weather`, theme format 0.23). It reads as a table, the sky in words. States morph by key, as every chart's do.
    - **Set aside: the forecast as the deck's own nodes,** each figure a quote. A refresh would have to rebind every icon, a week would be some 38 nodes, and a fixed stack cannot turn into rows in a 9:16 format.
20. **The request** (`scaena_objects::weather`, on call 1's answer; Open-Meteo's as written):
    - **The place.** Its geocoder, given a name or a postal code. A code shaped like a postal code is narrowed by the deck's region, since numeric codes repeat across countries: 75001 is both Paris and Addison, Texas. Coordinates skip it, and so does a refresh. It finds a code only where its place lists it: on 2026-10-15 it found 80331 (Munich) and 02139 (Cambridge, Massachusetts), and not 10115 (Berlin), 00184 (Rome), or 00-001 (Warsaw). PLAN O.2g asks elsewhere for those.
    - **The forecast.** `/v1/forecast` with `timezone=auto` and the units the deck reads in: seven daily variables over seven days, or seven hourly variables over one day. Seven variables and under two weeks keep a request at one call by Open-Meteo's count: two calls to insert, one to refresh.
    - **With the user's key,** the same requests go to the `customer-` hosts with `apikey`.
    - **The credit.** "Weather data by Open-Meteo.com", linked as its licence page asks, then "CC BY 4.0" linked to the licence and "location data by GeoNames". CC BY asks a change to be indicated too, so a forecast whose rows were edited adds "edited". It is drawn at the chart's foot after the place and the forecast's date at the place.
21. **W428 for a forecast:** its first period's day has passed at its place (`now` plus the stored offset), unless it is `kept`. Refresh replaces the rows and the record from today at the place, takes away a highlight whose period is gone, and says so.

### O.3 The price chart: a series kept as data, drawn as the line chart there is

22. **The series.** A CSV under `data/`, named for the symbol: `time` (civil, in the exchange's zone: a day, or an intraday close at its bar's end), `close` (split-adjusted), and `base` (the close the change counts from). The record is `fetched.stock`: `symbol`, `range` (`1d`, `1w`, `1m`, `3m`, `1y`, `ytd`), `exchange`, `currency`, `zone`, and `through`, the last price's civil time. Opens, highs, lows, and volume are not kept: the line draws closes, and a deck shared carries less of the provider's data.
23. **The chart** is a line on a time axis (PLAN 2.81), lit in the theme's signal color (G.19).
    - **What it draws.** The first and last closes are printed on the line, the value axis is off, and a day's chart has a rule at the previous close. There is no green and no red: the theme draws it.
    - **The highlight.** A `highlight` whose `at` names nothing lights every mark (deck format 0.34), so a lit line stops listing its every x.
    - **The claim** is quoted (ADR-0019) through one transform that takes the last close and the base and derives the change and its way. A figure place gets "+12.4%", and a headline reads "AAPL is [up] [12.4%] over three months". Every write sets the quoted words again.
    - **The credit** is drawn at the chart's foot: "Data provided by Twelve Data", linked, then "prices to Oct 15, 2026, 4:00 PM New York time".
24. **The request** (`scaena_objects::market`, on call 2's answer; Twelve Data's as written).
    - **One endpoint.** `/time_series`: 5-minute bars for a day, with the previous session's close, and daily closes for the rest. Prices are split-adjusted, in the exchange's local time, at one or two credits a fetch.
    - **The key** goes in an `Authorization: apikey` header.
    - **The window** is reckoned from the answer, never the clock, so a plan asks the same on every machine.
    - **Refusals** are said in the service's words.
25. **W428 for prices:** `through` is older than its range allows (a day for 1D and 1W, a week for 1M, 3M, and the year to date, a month for 1Y), unless the series is `kept`.

### O.4 The haiku: written by the user's model, counted by the engine, set in a verse role

26. **Asked as the writing tools ask.** `scaena_core::writing` gains the haiku's words.
    - **What it is about:** a topic, the slide shown (its parts as Draft Notes reads them, and its beat's claim), or the whole talk.
    - **The whole talk:** the deck's title, each beat's claim in spine order, then the notes, cut to 6,000 characters. The deck's words go inside `<talk>` tags, said to be what the poem is about and never instructions.
    - **Who sends it.** The CLI, the Mac, and the iPad send it through `scaena_chat::once`; the browser sends it through the assistant's adapters, which the record of exchanges holds to the same words (ADR-0022).
    - **Who writes it.** The model the user's key reaches, on every client. On a Mac or an iPad with no key kept, Apple's on-device model writes it. The browser with no key says where one goes. This reverses the writing tools' order on purpose: those edit a person's own words, where privacy leads; a haiku is made for a room, where the model the person chose leads.
27. **Counted by the engine, never refused for its count.** `scaena_core::verse` counts in integers, with no dependency.
    - **English:** a table of the 2,000 most frequent words its rules miss, from CMUdict (BSD-style), ranked by Google Books Ngram (CC BY 3.0); 7,758 bytes gzipped. Then rules after words/syllable 5.0.1 (MIT). A word said two ways counts either.
    - **Japanese:** morae, from the kana reading the model gives after the poem.
    - **Any other language** is left uncounted.
    - **Asked once more.** A poem off 5-7-5 is asked for once more with its counts, and the closer of the two is offered, with a note in plain words.
    - **Why the engine counts.** In PhonologyBench (2024), the best of six models then tested counted a sentence's syllables exactly 55% of the time, people 90%. Newer models were not tried. The counter's own accuracy is the research pass's measurement.
28. **Set as written.**
    - **The `verse` role.** The shipped themes gain it: the display face upright, 64 cu (Ember's 48), leading 1.5, `wrap: none`, `minSize` 40 (Ember's 32).
    - **`wrap: none`** on a text or a role (deck format 0.35, theme format 0.24) breaks a line only where the text does, and `fit: shrink` shrinks the whole poem rather than break a long line.
    - **A Haiku layout** in Statements: the poem centered, a short rule under it, and an attribution that waits empty.
    - **One patch.** Taken, it is one patch and one step to undo.
29. **Its provenance is in the notes, not on the slide.** The state's notes say which model wrote it, with whose key, when, and about what. No text, description, or attribution on the slide names the model, and the attribution is never filled in with a person's name for them.
30. **I404** (new, info): a verse text of three lines, in English or in kana alone, that does not count 5-7-5. It carries no fix, since a poem's words are its author's. The MCP tool `deck_haiku` calls no model: without lines it answers what to ask, and with lines it checks and places them.

## Calls for Jay

### 1. Open-Meteo's terms for commercial use

Open-Meteo's free API is for non-commercial use only. Its terms count as commercial use apps with subscriptions or advertisements, and "integrating our service into commercial products or promotional activities". They define non-commercial use as Creative Commons does, so a deck made for a business is likely commercial use, whatever Scaena's own model. Scaena's repository states no business model. The call holds PLAN O.2c–O.2e and O.2g; the forecast's format and chart (O.2a, O.2b, O.2f) need no answer.

| Option | Cost | Terms |
|---|---|---|
| **A.** The free API, keyless, non-commercial use only | $0 | Non-commercial only: no subscriptions or advertising, not in commercial products or promotional activities. Under 10,000 calls a day, 5,000 an hour, 600 a minute, and 300,000 a month. CC BY 4.0, credited beside the data. No uptime guarantee, and misuse blocked without notice. |
| **B.** A subscription of the publisher's, its key built into every client | Standard: $29 a month or $319 a year, 1,000,000 calls a month. Professional: $99 a month or $1,099 a year, 5,000,000. Enterprise: by contact. | A commercial licence on the `customer-` hosts, with a 99.9% uptime target. The subscriber may not share account or login information with anyone. With no server of Scaena's, the key would ship in every client, where anyone could copy it: against those terms and invariant 7. |
| **C.** Keyless for non-commercial use, and the user's own key for commercial use (recommended) | $0 to the publisher. A commercial user pays Open-Meteo at least $29 a month, and most decks made for work are commercial use. | A's terms without a key. With a key, the user's own commercial licence on the `customer-` hosts, which take the same requests plus `apikey`. CC BY 4.0 credit either way. |
| **D.** OpenWeather's One Call 4.0, the user's key (its pricing page marks 3.0 deprecated) | The first 1,000 calls a day free, then $0.0015 a call, held to 2,000 a day by default. Subscribing takes a billing form, even for the free calls. | Commercial use allowed. On its Free to Professional plans, "Weather data provided by OpenWeather", a link, and its logo shown with the data. Its own condition ids (a second sky table), 48 hours by the hour, and 8 days by the day. |
| **E.** MET Norway's Locationforecast, keyless | $0 | NLOD 2.0 and CC BY 4.0, credited. Requests name the app: a User-Agent with a contact, or a browser's Origin. Over 20 requests a second needs an agreement. Its terms (revised 26 June 2020) say browsers and apps should go through a proxy of their own, and must once their traffic is heavy. No SLA. Its compact series is in UTC and gives no chance of rain. |
| **F.** Open-Meteo's open server, hosted by Scaena | A server's running costs | AGPLv3. A server of Scaena's: excluded by invariant 7. |

**Recommendation: C.** Before Scaena sells subscriptions or shows advertising, write to Open-Meteo (info@open-meteo.com, the address its terms give), describe a presentation app whose users' own devices make two calls a forecast, and record the answer here. If it does not cover a keyless default in a paid Scaena, require the user's key there.

If a deck made for work should have a forecast that costs its author nothing, D is that path: PLAN O.2h, optional, behind `fetched.from`. It would place the forecast by Geoapify's geocoder with the map's key, since Open-Meteo's free geocoder is non-commercial too.

**Why:**
- **One provider and one path serve both kinds of use:** the `customer-` hosts take the same requests, and only the host and `apikey` differ.
- **Open-Meteo fits the object best.** It gives WMO codes, days cut at the place's own midnight, the chance of rain, and units on request. Its geocoder reads cities and some postal codes. Its four hosts answer any origin, so every client calls it directly.
- **No key of the publisher's goes into any client**, and the publisher pays nothing.
- **Each alternative costs more to build.** D adds a second sky table and a logo on the slide. E adds days worked out from UTC, a second geocoder, a proxy its terms ask for, and no chance of rain.
- **The trade.** For a deck made for work, C costs its author $29 a month where D costs nothing within 1,000 calls a day.

### 2. The market-data provider

None of the services read licenses its free tier for showing prices to an audience, and none publishes a plan that plainly covers what a deck does with a series: shows it to a room, goes to others with the series inside it, and keeps it in its history. The call holds PLAN O.3b, O.3d, O.3f, and O.3g; the series' format, its chart, and its layout (O.3a, O.3c, O.3e) need no answer.

| Service | Free tier | Showing prices to others | Terms | From a browser |
|---|---|---|---|---|
| **Twelve Data** (recommended) | Basic, $0: 8 credits a minute, 800 a day; every range from 5-minute bars, at 1 or 2 credits a fetch. "Internal non-display usage": "for testing, evaluation, or development purposes only", never "displayed to users". | Its individual plans (Grow from $29) are for one person's own use, display "limited to internal personal use", and a company needs a business plan even inside. Venture, from $149 a month, 17% less billed yearly: "external display". Enterprise, from $1,099: "external distribution". | Terms of 1 January 2026: giving its data to third parties is Redistribution (§1), allowed only by an add-on, a written agreement, or a plan that grants it (§2.2(e)), and its data is deleted within 30 days of a subscription's end (§16.2). Attribution wherever its data is shown publicly: "Data provided by Twelve Data", linked, near the data. | Answers any origin; its preflight allows `Authorization`, so the key goes in a header |
| **Alpha Vantage** | $0: 25 requests a day; daily closes for the latest 100 sessions, as traded; weekly adjusted for 25 years; intraday is premium, so no day by the 5 minutes | Only by written agreement (premium@alphavantage.co), no price published | Personal and non-commercial. Its terms count as commercial any use on behalf of a company, and any that lets others reach the data. Premium plans ($49.99–$249.99 a month) lift its limits, not those terms. No attribution found. | Answers any origin; the key goes in the query |
| **Massive** (Polygon.io, renamed 2025-10-30) | Stocks Basic, $0: 5 calls a minute, 2 years, end of day, minute aggregates | Stocks Business, $2,499 a month, as soon as anyone but the subscriber sees the data, charts derived from it included | Individual plans are personal and non-business | Echoes the origin; `Authorization: Bearer` allowed |
| **Finnhub** | Not read: its pricing page and its candles' documentation render by script (the research pass read 60 calls a minute, and candles premium) | No published price read | Personal use; no sharing of its data, or results derived from it, without written approval | Answers any origin |
| **Marketstack** | $0: 100 requests a month, end of day, non-commercial | Its paid plans list "Commercial Use" from $9.99 a month, intraday included; its US prices are licensed from Tiingo | Not read: its terms are apilayer's, on a page that renders by script. Whether its commercial use covers a room or a shared deck is unverified. | Answers any origin; the key goes in the query (`access_key`) |
| **None:** the person brings the series | $0 | As the person's own source allows | The person's own: pasted cells or a CSV, as a data source is attached today | Nothing fetched |

**Recommendation: Twelve Data**, behind a table of services, with its fetching shipped only once Twelve Data answers in writing (below). Until then a price chart is made from a series the person brings, which O.3a, O.3c, and O.3e build. Alpha Vantage can follow for decks a person keeps to themself (PLAN O.3i).

**Why:**
- **Every range Jay asked for**, a day by the 5 minutes included, on its free plan, from one endpoint.
- **Civil times with no table of zones.** Its prices are split-adjusted, in the exchange's local time, which fits Scaena's civil dates, so every client and CI build and test it on recorded answers.
- **The lowest published price for an audience.** Venture, from $149 a month, lets prices be shown to people outside the company. Alpha Vantage needs a written agreement at no published price, Massive costs $2,499 a month, and Finnhub forbids sharing. Marketstack lists commercial use from $9.99 a month, but its terms could not be read.
- **Its attribution rule is concrete,** and the engine's credit meets it.
- **The key never rides in a URL,** since it goes in a header from any page.

**The cost of the choice.** Twelve Data's terms were written for apps, not documents:
- **A room.** Showing prices to people takes a business plan, Venture from $149 a month. The editors say so where its key is added.
- **A deck sent to others.** It carries its series in a data file. Giving data to third parties is Redistribution, which Venture's "external display" does not name. Enterprise's "external distribution" ($1,099 a month), an add-on, or a written agreement does.
- **A deck kept.** Its data is deleted within 30 days of a subscription's end. A deck's history keeps every version of a data file (ADR-0014), and nothing takes one out of a history.

So before any fetching ships (O.3d, O.3f, O.3g), ask Twelve Data in writing: whether a deck that keeps a series, is shown, and is shared is display or redistribution; and whether a series kept in a deck and its history may outlive the subscription. Its terms name sales@twelvedata.com for redistribution and external display (§2.4). Ask Marketstack the same, since its price is the lowest listed. Record both answers here.

## Consequences

- **+** One record, one plan shape, one credit, and one date rule serve three objects, and the haiku is one more writing task. Every client asks and patches through the same Rust.
- **+** A deck with a map, a forecast, or a price chart draws the same offline, in a single file, in the PDF, and in video, on every painter, held by the goldens. Fetching again is an ordinary, undoable edit in the history.
- **+** A map and a non-commercial forecast cost the user nothing to start: Geoapify's Free plan and Open-Meteo's free API. A haiku costs about a cent with a mid-priced model, and less than a tenth of a cent with the cheapest the providers list (their published prices, read 2026-10-15).
- **−** A map's picture is the provider's, not the theme's. A re-theme from light to dark leaves its style until Fetch Again; only the pin, its label, and the credit follow the theme.
- **−** A user needs a Geoapify key for a map. To present prices, a user needs Twelve Data's Venture plan, and to share a deck that carries them, more than Venture names (call 2). Scaena cannot see a user's plan, so it says what each plan allows where the key is added.
- **−** A deck's history keeps every version of a data file (ADR-0014), and nothing takes one out of a history. A service whose terms ask that its data be deleted when a subscription ends cannot be met by a deck that keeps a history.
- **−** Two credits show on a map from Geoapify: its own in the picture's bottom-right corner, and Scaena's in the bottom left. Only Scaena's is legible at every size, linked, and read wherever the map is shown.
- **−** The MCP server reaches the network for the first time. Its tools say what they send where, and an agent's host may have to allow it.
- **−** What is typed goes to the service: an address to Geoapify, a place to Open-Meteo, a ticker to Twelve Data, and the talk's claims and notes to the model's provider. Each sheet says so before it asks, and coordinates skip a geocoder.
- **−** The editor's module may run out of room (decision 9). Tables are handed over before any budget moves.
- **−** It takes five deck formats (0.31–0.35, and 0.36 if the optional closed axis lands) and four theme formats (0.21–0.24). Each deck bump rewrites the decks that name the format, and each theme bump the themes.
- **−** Terms move. Stadia's change on 2026-11-09, Open-Meteo's take effect when posted, Twelve Data's were updated on 1 January 2026, and OpenWeather has moved One Call to 4.0. Each adapter rereads its service's terms when it is touched, and recorded answers catch an answer whose shape drifts.

## Alternatives

- **A map drawn by the engine** from OpenStreetMap's features (Nominatim for the place; OpenFreeMap's vector tiles), kept in the bundle and colored by the theme. Not chosen (Decided). It would also have leaned on two donated services with no SLA, and on Nominatim's one request a second for the whole app.
- **Apple's MapKit snapshots** on the Mac and the iPad, which need no key. Apple's terms forbid storing Map Data beyond a temporary basis and publishing it (decision 10).
- **MapTiler's or Mapbox's static maps.** MapTiler forbids keeping static images. Mapbox asks that what was downloaded be destroyed when the licence ends, and its unauthorized answer sends no CORS header (decision 10).
- **Apple's WeatherKit** on the Mac and the iPad. Attachment 8 of Apple's developer agreement, §1.6, lets Apple Weather Data be stored only on a temporary and limited basis, and a deck keeps its forecast.
- **A server of Scaena's,** to pool and cache requests or to host Open-Meteo's server. Invariant 7.
- **A key of the publisher's in every client.** It would be shared with anyone who looks, against the services' terms and invariant 7.
- **Fetching at render,** a live source refreshed as a deck plays. That breaks determinism, fails offline, and leaves nothing for the PDF; SPEC §16 Q4 stays open.
- **The forecast as the deck's own nodes** (decision 19), and **the credit as an ordinary caption node.** A person could delete the caption or move it off the slide, and every client would have to keep it with its object.
- **Shipping CMUdict whole:** 363,489 bytes gzipped, more than twice the editor's room. **Trusting the model's count:** the best of 2024's models was wrong on about half the sentences PhonologyBench asked.

## Sources

Each page was opened on 2026-10-15 through the session's proxy, by the research pass and again by the second reader, who corrected what a page did not say. A page that could not be opened, or that renders its text by script, is marked unverified, with the claim it would have carried.

**Maps**

| Source | What it gave |
|---|---|
| https://www.geoapify.com/static-maps-api/ | Static maps may be "freely" cached, stored, and redistributed, with OpenStreetMap's and other contributors' credit; the print version may be used in commercial products at no extra cost; 4,096 × 4,096 px, larger on request |
| https://apidocs.geoapify.com/docs/maps/static/ | Its parameters (`apiKey`, `style`, `width`, `height`, `center`, `zoom` 1–20, `scaleFactor` 1–2, `format`); at scale factor 2, width and height each at most 4,096; the credit it asks (Geoapify on the Free plan, OpenStreetMap, OpenMapTiles), written in the picture's bottom-right corner by default; print credit beside the map or in acknowledgments; cost, 1 credit plus a credit for each four 256-px tiles, its example counting 15–20 tiles for 1,200 × 600 |
| https://www.geoapify.com/pricing/ | Free: 3,000 credits a day, up to 5 requests a second; paid plans from $59 a month; commercial use on the Free plan in production, with the credits |
| https://www.geoapify.com/terms-and-conditions/ | Version 5, 2 February 2024: OpenStreetMap's credit always; Geoapify's on the Free plan; Free's commercial use "with some limitations" in production, by contact. Nothing on keeping maps |
| https://apidocs.geoapify.com/docs/geocoding/forward-geocoding/ | Its geocoder: `text`, `lang` (ISO 639-1), `limit`, filters; one credit a request; each result's `datasource`, with its credit and licence; a key may be held to origins |
| https://www.geoapify.com/geocoding-api/ | Its geocoding results may be stored, with their sources credited |
| https://maps.geoapify.com/v1/staticmap (unauthorized, with an Origin header, and its preflight) | 401 with `Access-Control-Allow-Origin: *`, the same from `api.geoapify.com/v1/geocode/search`; the preflight allows `authorization` and `x-api-key`. CORS on an authorized answer is unverified |
| https://docs.stadiamaps.com/static-maps/ | The `static_cacheable` endpoint (kept for digital use while subscribed; printed up to 5,000 copies); sizes 256–4,096 points with `@2x`; zoom 0–18; `manual_attribution`; the key in a query or an `Authorization: Stadia-Auth` header |
| https://stadiamaps.com/pricing/ | Free (no commercial use, no static maps); Starter $20 a month, 1,000,000 credits, 3¢ per 1,000 more; Standard $80; Professional $250; a static map 20 credits, a cacheable one 2,000, a forward geocode 20; geocoding results kept persistently from Standard |
| https://stadiamaps.com/terms-of-service/ | Terms in force from 2026-03-18: the Cacheable Static Maps Addendum (print up to 5,000 copies an image; digital use while a paid subscription is kept); no sharing of keys or subscriptions; the free tier for non-commercial use or evaluation |
| https://stadiamaps.com/terms-of-service/versions/2026-11-09/ | From 2026-11-09: "Digital Embeddable Usage", where "the original image can not be served", embedded for good and redistributed up to 5,000 copies; "Digital Cacheable Usage" while a paid subscription lasts; geocoding results stored while on Standard or above; "Commercial Use" defined |
| https://docs.stadiamaps.com/attribution/ | "© Stadia Maps © OpenMapTiles © OpenStreetMap", linked; in print and video, the URLs printed |
| https://tiles.stadiamaps.com/static_cacheable/alidade_smooth.png (unauthorized, and its preflight) | 401 and 204 with `Access-Control-Allow-Origin: *` and `Access-Control-Allow-Headers: Stadia-Auth,Content-Type` |
| https://developer.apple.com/support/terms/apple-developer-program-license-agreement/ | The Apple Maps Service includes "related tools for capturing map content (e.g., MapSnapshotter)"; Attachment 6, §2.3, §2.4, and §2.5; Attachment 8 (WeatherKit), §1.6 |
| https://developer.apple.com/tutorials/data/documentation/mapkit/mkmapsnapshotter.json | MKMapSnapshotter's documentation: it captures map tiles and imagery into an image, and grants no right to keep one |
| https://www.maptiler.com/terms/ and https://www.maptiler.com/cloud/terms/ | The Cloud terms (served at maptiler.com/terms/cloud/): no storing, saving, or redistributing map content "by using a screenshot or other static image"; "Limited Prints" for an internal purpose, up to A4 at 300 DPI; results cached only temporarily, for one end user |
| https://docs.mapbox.com/api/maps/static-images.md and https://www.mapbox.com/legal/tos | 1,280 px a side; attribution required. Its terms of service (31 March 2024): "Upon any termination of the limited license, you agree to immediately destroy any materials or other data or content provided by, downloaded from, or otherwise accessed through the Services." Its unauthorized answer sent no CORS header. Its product terms page renders by script: unverified |
| https://osmfoundation.org/wiki/Licence/Attribution_Guidelines | Credit "OpenStreetMap", linked to openstreetmap.org/copyright to make the ODbL clear; for PDFs and print, "the URL to openstreetmap.org/copyright must be printed out"; one credit serves several static images in one document |
| https://operations.osmfoundation.org/policies/nominatim/, https://openfreemap.org/, and https://openfreemap.org/tos/ | Read by the research pass alone, for the vector map not chosen: Nominatim's one request a second for a whole app; OpenFreeMap's free tiles with no SLA |

**Weather**

| Source | What it gave |
|---|---|
| https://open-meteo.com/en/terms | Non-commercial use only on the free API, defined as Creative Commons defines it, with its examples of both kinds of use; under 10,000 calls a day, 5,000 an hour, 600 a minute, 300,000 a month; CC BY 4.0; misuse blocked without notice; no sharing of account or login information; changes in force when posted; the contact address |
| https://open-meteo.com/en/licence | Credit, a link to the licence, and changes indicated (CC BY 4.0); "Weather data by Open-Meteo.com", linked, next to the data; GeoNames for places; AGPLv3 server |
| https://open-meteo.com/en/pricing | The `customer-` hosts and `apikey`; more than 10 variables or two weeks counts as more than one call; a 99.9% uptime target; the free tier for "evaluation and prototyping". The page's own text shows no prices |
| https://merchant-ui-api.stripe.com/pricing-table/prctbl_1MJg0iLNZMSyEuRUvpoeTHOb | The price table the pricing page draws: Standard $29 a month or $319 a year, Professional $99 or $1,099, Enterprise by contact |
| https://open-meteo.com/en/docs/geocoding-api | The geocoder: `name` (a place or a postal code, matched by prefix), `count`, `language`, `countryCode`. The forecast's documentation was read by the research pass alone: its variables, `timezone=auto`, units, errors, and the WMO codes |
| https://geocoding-api.open-meteo.com/v1/search and https://api.open-meteo.com/v1/forecast | Live answers: `Access-Control-Allow-Origin: *` on all four hosts, and 400 with its `reason` for a bad key. Postal codes found: 75001, 80331, 02139; not 10115, 00184, 00-001, M5V, 2000 (Australia), or 110001 (India). When a chance of rain comes back null is unverified |
| https://download.geonames.org/export/zip/readme.txt and https://api.zippopotam.us/de/10115 | GeoNames' postal codes: CC BY 4.0, the United Kingdom's with Royal Mail's data. Zippopotam.us answered 10115 with `Access-Control-Allow-Origin: *`; its home page answered empty, so its terms are unverified |
| https://openweathermap.org/price, https://openweathermap.org/api/one-call-3, and https://openweathermap.org/faq | One Call 4.0 by the call: 1,000 calls a day free, then $0.0015, 2,000 a day by default, after a billing form; 3.0 marked deprecated; commercial use allowed; on its Free to Professional plans, "Weather data provided by OpenWeather", a link, and its logo |
| https://docs.api.met.no/doc/TermsOfService and https://docs.api.met.no/doc/License | Revised 26 June 2020: a User-Agent with a contact, or a browser's Origin; over 20 requests a second by agreement; browsers and apps through a proxy of their own, which heavy traffic must use; NLOD 2.0 and CC BY 4.0, credited to MET Norway |

**Market data**

| Source | What it gave |
|---|---|
| https://twelvedata.com/pricing and https://twelvedata.com/pricing.md | Individual plans are for "personal, internal, and non-commercial use", their display "limited to internal personal use"; a business plan is "required for any usage where Twelve Data is consumed on behalf of a business". Basic: "internal non-display usage", "for testing, evaluation, or development purposes only… cannot be displayed to users, shared externally, or used in production systems"; Grow from $29; Pro from $99; 17% off billed yearly |
| https://twelvedata.com/pricing-business | Venture, from $149 a month: "external display data access", "for companies showcasing data on client-facing apps or websites"; Enterprise from $1,099: "external distribution market data" |
| https://twelvedata.com/terms | Last updated 1 January 2026: Redistribution is "any publication, distribution, or provision of Data to third parties" (§1); redistribution and external display only as a tier, an add-on, or a written agreement grants (§2.2(e)), through sales@twelvedata.com (§2.4); data kept only for the subscription's term, and deleted within 30 days of its end (§16) |
| https://twelvedata.com/docs | `/time_series` at 1 credit a symbol; `adjust` is `splits` by default; `timezone` is the exchange's by default, and daily data comes in its local time; `previous_close`; the key in an `Authorization: apikey` header (recommended) or the query |
| https://support.twelvedata.com/en/articles/12647398-attribution-guidelines-for-using-twelve-data | Of 12 January 2026: "Data provided by Twelve Data", a dofollow link, near the data; api@twelvedata.com for licensing questions. Its help article on the depth of history answered 404: unverified |
| https://www.alphavantage.co/premium/, https://www.alphavantage.co/documentation/, and https://www.alphavantage.co/terms_of_service/ | 25 requests a day; premium $49.99–$249.99 a month; the free daily series is the latest 100 points, as traded; intraday premium. Its terms, a PDF: personal, non-commercial use, commercial use counted as any on behalf of a company or any that lets others reach the data, by agreement at premium@alphavantage.co |
| https://massive.com/pricing, https://massive.com/business, and https://massive.com/knowledge-base/article/which-plan-do-i-need-to-show-massive-data-in-my-app | Stocks Basic free (5 calls a minute, 2 years, end of day, minute aggregates); Stocks Business $2,499 a month, needed "as soon as anyone other than you can see the data"; derived values covered by it |
| https://finnhub.io/terms-of-service | Personal use; no redistributing or sharing its data or "derived results from the data" without written approval. Its pricing page and https://finnhub.io/docs/api/stock-candles render by script: the free tier's limits and candles' price are unverified |
| https://marketstack.com/pricing and https://marketstack.com/faq | Free: 100 requests a month, end of day, non-commercial; Basic $9.99 a month, Professional $49.99, Business $149.99, each listing "Commercial Use"; US prices licensed from Tiingo; the key appended to the URL. Its terms (apilayer's, at ideracorp.com) render by script: unverified |
| api.twelvedata.com, www.alphavantage.co, finnhub.io, api.massive.com, and api.marketstack.com (unauthorized, with an Origin header) | Each answered with `Access-Control-Allow-Origin`; Twelve Data's preflight allows `Authorization` |

**The haiku**

| Source | What it gave |
|---|---|
| https://arxiv.org/html/2404.02456v1 | PhonologyBench (2024): the best of six models, Claude 3 Sonnet, counted a sentence's syllables exactly 55.3% of the time, a human 90.0% |
| https://www.hsa-haiku.org/hsa-definitions.html and https://www.hsa-haiku.org/hsa-definitions-2004.html | The Haiku Society of America: its definition counts no syllables; most English haiku are seventeen syllables or fewer |
| https://raw.githubusercontent.com/cmusphinx/cmudict/master/README and its LICENSE | CMUdict: use for any purpose unrestricted, its origin acknowledged; BSD-style |
| https://unpkg.com/syllable@5.0.1/readme.md | words/syllable 5.0.1, MIT |
| https://storage.googleapis.com/books/ngrams/books/datasetsv3.html | Google Books Ngram: CC BY 3.0 |
| https://en.wikipedia.org/wiki/Mora_(linguistics) | How kana are counted as morae |
| https://developers.openai.com/api/docs/pricing and https://ai.google.dev/gemini-api/docs/pricing | Published prices a million tokens, behind the haiku's cost under Consequences: $2.00 in and $10.00 out for a mid-priced model, and as little as $0.10 in and $0.50 out (OpenAI's page, opened again; Gemini's read by the research pass) |
| https://www.anthropic.com/legal/aup and https://developer.apple.com/support/terms/acceptable-use-requirements-for-the-foundation-models-framework | Anthropic's policy bars content that misleads "audiences about its origin" and AI-assisted work submitted "without proper permission or attribution"; the notes carry a haiku's provenance. Nothing in Apple's list bars a short poem |

The research pass's own measurements, not a source's, are unverified as such: how often the counter is right (97% of running words by its rules, 99.9% with the table, against CMUdict), the bytes of its table and the icons' paths, and the estimates in decision 9.
