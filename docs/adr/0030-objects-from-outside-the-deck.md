# ADR-0030: Objects from outside the deck: kept in the bundle, refreshed by an edit, credited, and drawn in the theme

**Status:** proposed · **Date:** 2026-10-15

## Context

Jay asked for four objects on 2026-10-15 (PLAN, "Objects from outside the deck", O.1–O.4):
- **A map:** "give an address and get back a map object."
- **A forecast:** "provide a city or zip and get back a day or week weather forecast."
- **A price chart:** a ticker symbol in, and a chart of a day, a week, a month, a quarter, a year, or the year to date out.
- **A haiku:** "provide either a topic or just point at the deck and ai writes a nice haiku and formats it beautifully."

Three come from a service outside the deck, and the fourth from the user's own model. A research pass read each candidate service's own pages on 2026-10-15; they are listed under Sources, and a claim that rests on a page that could not be opened says so.

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

   W428 (new) says when a fetched object has passed its date. A fetch is no patch, so the finding carries no `fix`: an optional `refresh` names what to fetch again (SPEC §7.4). Without `now`, no date is judged, so render, validation, the goldens, and every test without a clock stay as they are. None of it enters the CRDT.
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
    - **Keeping, printing, and the PDF.** Geoapify's static maps page says its maps may be "freely" cached, stored, and redistributed, and that their print version may be used in commercial products at no extra cost, with OpenStreetMap's credit. Its terms ask for OpenStreetMap's credit always and Geoapify's on the Free plan, and say nothing against keeping a map.
    - **Free tier.** 3,000 credits a day, up to 5 requests a second. Its pricing page allows commercial use on the Free plan, in production, with the credits. Its terms say that use in production comes "with some limitations" and to contact it for details, so Geoapify is asked, before the editors ship, whether a presentation app whose users each bring their own Free key fits.
    - **Cost of a map.** One credit, plus a credit for each four 256-px tiles of the picture: about 9 credits at 1,920 × 1,080, and about 33 if its scale factor's pixels are counted. Its documentation does not say which.
    - **CORS.** Its static map and geocoder answer any origin: `Access-Control-Allow-Origin: *`, observed on unauthorized answers.
    - **Its own credit.** It writes its credit into the picture's corner. A paid plan may have that hidden on request; Scaena draws the credit as well (decision 14).
    - **Its geocoder.** It takes the same key at one credit a request. Its results may be stored ("we do not restrict you from storing results") with their sources credited.

    Stadia Maps fits a user who pays for it:
    - Static maps need its Starter plan, $20 a month. Its cacheable endpoint costs 2,000 credits a map, so Starter's 1,000,000 credits are 500 maps a month, with more at 3¢ per 1,000 credits.
    - Under its terms in force (2026-03-18), a cacheable picture may be kept and embedded in documents while a paid subscription is kept, and printed up to 5,000 copies. From 2026-11-09, a picture embedded in a document may be redistributed up to 5,000 copies for good.
    - Its geocoding results may be stored for good only on its Standard plan ($80) and above.
    - Its CORS preflight allows `Stadia-Auth`, not `Authorization`, so a browser sends its key in the query.

    Two services are not offered:
    - **MapTiler.** Its terms forbid storing or redistributing map content as a static image, and allow only internal prints.
    - **Mapbox.** No clause on keeping a static picture was found in its terms, and its unauthorized answer sends no CORS header.

    **Apple's MapKit snapshots are not used on the Mac and the iPad.** The Apple Developer Program License Agreement defines the Apple Maps Service to include "related tools for capturing map content (e.g., MapSnapshotter)", and its Attachment 6 holds that service's Map Data, imagery among it:
    - §2.5 lets Map Data be cached, pre-fetched, or stored only "on a temporary and limited basis", then deleted, unless Apple permits otherwise in writing or in MapKit's documentation.
    - §2.3 forbids copying, publishing, or publicly displaying Map Data except as it expressly permits.
    - MKMapSnapshotter's documentation permits nothing more.

    A deck keeps its map, shows it to a room, and exports it, so a snapshot cannot be its picture. One service serves every client.
11. **The request** (`scaena_objects::map`):
    - **The place.** Geoapify's `geocode/search` (`text`, `lang` the deck's language, `limit=5`, `format=json`), skipped for coordinates typed as `lat, lon`. Several places found are offered to pick from.
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
    - **Where it goes.** On a card in the picture's bottom corner (`credit: "inside"`, the default), or as a line under the picture inside the node's box (`credit: "below"`).
    - **Geoapify's own.** The credit Geoapify writes into the picture's corner on the Free plan stays where it is.
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
    - **`scaena_core::weather`** holds the WMO table: each code Open-Meteo documents, to a sky, its icon by day and by night, and its words in `docs/spec/format.md`'s six languages.
19. **The weather chart**, a tenth chart kind, `weather` (deck format 0.33).
    - **Its columns.** Each period is a column across the box, or a row down it where the box is taller than wide (a 9:16 format), and one period alone is drawn large.
    - **What a column holds:** the period's name (the weekday, or the hour as the time axis prints hours), the sky's icon, the high, the low in the muted color, and the chance of rain with a drop where it is 10% or more. The sky's words and the wind are parts a person can add.
    - **The icons** are Phosphor's regular `sun`, `cloud-sun`, `cloud`, `cloud-fog`, `cloud-rain`, `cloud-snow`, `cloud-lightning`, `moon`, `cloud-moon`, `drop`, and `wind`, compiled in by `scripts/build_icons.py`. An hour of night takes the moon.
    - **Units belong to the view.** The chart converts from the source's units at layout, in IEEE arithmetic the same on every target, so a change of units fetches nothing.
    - **The rest.** A `highlight` picks a period. The parts take the chart's roles until a theme names its own (`charts.weather`, theme format 0.23). It reads as a table, the sky in words. States morph by key, as every chart's do.
    - **Set aside: the forecast as the deck's own nodes,** each figure a quote. A refresh would have to rebind every icon, a week would be some 38 nodes, and a fixed stack cannot turn into rows in a 9:16 format.
20. **The request** (`scaena_objects::weather`, on call 1's answer; Open-Meteo's as written):
    - **The place.** Its geocoder, given a name or a postal code. A code shaped like a postal code is narrowed by the deck's region, since numeric codes repeat across countries: 75001 is both Paris and Addison, Texas. Coordinates skip it, and so does a refresh.
    - **The forecast.** `/v1/forecast` with `timezone=auto` and the units the deck reads in: seven daily variables over seven days, or seven hourly variables over one day. Seven variables and under two weeks keep a request at one call by Open-Meteo's count: two calls to insert, one to refresh.
    - **With the user's key,** the same requests go to the `customer-` hosts with `apikey`.
    - **The credit.** "Weather data by Open-Meteo.com", linked as its licence page asks (CC BY 4.0), and "location data by GeoNames". It is drawn at the chart's foot after the place and the forecast's date at the place.
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
    - **Why the engine counts.** Models count syllables worse than this counter: PhonologyBench's best model counted a sentence exactly 55% of the time, a human 90%.
28. **Set as written.**
    - **The `verse` role.** The shipped themes gain it: the display face upright, 64 cu (Ember's 48), leading 1.5, `wrap: none`, `minSize` 40 (Ember's 32).
    - **`wrap: none`** on a text or a role (deck format 0.35, theme format 0.24) breaks a line only where the text does, and `fit: shrink` shrinks the whole poem rather than break a long line.
    - **A Haiku layout** in Statements: the poem centered, a short rule under it, and an attribution that waits empty.
    - **One patch.** Taken, it is one patch and one step to undo.
29. **Its provenance is in the notes, not on the slide.** The state's notes say which model wrote it, with whose key, when, and about what. No text, description, or attribution on the slide names the model, and the attribution is never filled in with a person's name for them.
30. **I404** (new, info): a verse text of three lines, in English or in kana alone, that does not count 5-7-5. It carries no fix, since a poem's words are its author's. The MCP tool `deck_haiku` calls no model: without lines it answers what to ask, and with lines it checks and places them.

## Calls for Jay

### 1. Open-Meteo's terms for commercial use

Open-Meteo's free API is for non-commercial use only. Its terms count as commercial use apps with subscriptions or advertisements, and "integrating our service into commercial products or promotional activities". Scaena's repository states no business model. The call holds PLAN O.2c–O.2e and O.2g; the forecast's format and chart (O.2a, O.2b, O.2f) need no answer.

| Option | Cost | Terms |
|---|---|---|
| **A.** The free API, keyless, non-commercial use only | $0 | Non-commercial only: no subscriptions or advertising, not in commercial products or promotional activities. Under 10,000 calls a day, 5,000 an hour, 600 a minute, and 300,000 a month. CC BY 4.0, credited beside the data. No uptime guarantee, and misuse blocked without notice. |
| **B.** A subscription of the publisher's, its key built into every client | Standard: $29 a month or $319 a year, 1,000,000 calls a month. Professional: $99 a month or $1,099 a year, 5,000,000. Enterprise: by contact. | A commercial licence on the `customer-` hosts, with a 99.9% uptime target. The subscriber may not share account or login information with anyone. With no server of Scaena's, the key would ship in every client, where anyone could copy it: against those terms and invariant 7. |
| **C.** Keyless for non-commercial use, and the user's own key for commercial use (recommended) | $0 to the publisher; a commercial user pays Open-Meteo from $29 a month | A's terms without a key. With a key, the user's own commercial licence on the `customer-` hosts, which take the same requests plus `apikey`. CC BY 4.0 credit either way. |
| **D.** OpenWeather's One Call, the user's key | The first 1,000 calls a day free, then $0.0015 a call, capped at 2,000 a day by default | Commercial use allowed. On its Free to Professional plans, its name, a link, and its logo shown with the data. Its own condition ids (a second sky table), 48 hours by the hour, and 8 days by the day. |
| **E.** MET Norway's Locationforecast, keyless | $0 | NLOD 2.0 and CC BY 4.0, credited. Requests name the app: a User-Agent with a contact, or a browser's Origin. Over 20 requests a second needs an agreement, and a caching proxy is preferred. No SLA. Its compact series is in UTC and gives no chance of rain. |
| **F.** Open-Meteo's open server, hosted by Scaena | A server's running costs | AGPLv3. A server of Scaena's: excluded by invariant 7. |

**Recommendation: C.** Before Scaena sells subscriptions or shows advertising, write to Open-Meteo (info@open-meteo.com, the address its terms give), describe a presentation app whose users' own devices make two calls a forecast, and record the answer here. If it does not cover a keyless default in a paid Scaena, require the user's key there, and keep D or E for later behind `fetched.from`.

**Why:**
- **One provider and one path serve both kinds of use:** the `customer-` hosts take the same requests, and only the host and `apikey` differ.
- **Open-Meteo fits the object best.** It gives WMO codes, days cut at the place's own midnight, the chance of rain, and units on request. Its geocoder reads cities and many postal codes. Its four hosts answer any origin, so every client calls it directly.
- **No key of the publisher's goes into any client**, and the publisher pays nothing.
- **Each alternative costs more.** D adds a second sky table and a logo on the slide. E adds days worked out from UTC, a second geocoder, a proxy its terms prefer, and no chance of rain.

### 2. The market-data provider

None of the four services read licenses its free tier for showing prices to an audience. The call holds PLAN O.3b, O.3d, O.3f, and O.3g; the series' format, its chart, and its layout (O.3a, O.3c, O.3e) need no answer.

| Service | Free tier | Showing prices to others | Terms | From a browser |
|---|---|---|---|---|
| **Twelve Data** (recommended) | Basic, $0: 8 credits a minute, 800 a day; every range from 5-minute bars, at 1 or 2 credits a fetch; "for testing, evaluation, or development purposes only", never "displayed to users" | Venture, $149 a month ($124 billed yearly): external display to end users. Grow, from $29: view-only display inside one's own apps. Enterprise, from $1,099: distribution. | Attribution wherever its data is shown publicly: "Data provided by Twelve Data", linked, near the data. Free-tier data never for commercial purposes. | Answers any origin; its preflight allows `Authorization`, so the key goes in a header |
| **Alpha Vantage** | $0: 25 requests a day; daily closes for the latest 100 sessions, as traded; weekly adjusted for 25 years; intraday is premium, so no day by the 5 minutes | Commercial use by contacting its sales, no price published; premium plans, $49.99–$249.99 a month, are for personal use | Personal and non-commercial; no attribution found in its terms | Answers any origin; the key goes in the query |
| **Massive** (Polygon.io, renamed 2025-10-30) | Stocks Basic, $0: 5 calls a minute, 2 years, end of day, minute aggregates | Stocks Business, $2,499 a month, as soon as anyone but the subscriber sees the data, charts derived from it included | Individual plans are personal and non-business | Echoes the origin; `Authorization: Bearer` allowed |
| **Finnhub** | $0: 60 calls a minute, personal use; candles are premium, so no chart | No published price read | Personal use; no sharing of its data, or results derived from it, without written approval | Answers any origin |

**Recommendation: Twelve Data**, behind a table of services, so Alpha Vantage can follow for decks a person keeps to themself (PLAN O.3i).

**Why:**
- **Every range Jay asked for**, a day by the 5 minutes included, on its free plan, from one endpoint.
- **Civil times with no table of zones.** Its prices are split-adjusted, in the exchange's local time, which fits Scaena's civil dates, so every client and CI build and test it on recorded answers.
- **The only published plan for an audience.** It has the one published plan that lets prices be shown to people outside the company (Venture, from $149 a month), where Alpha Vantage needs a sales call, Massive costs $2,499 a month, and Finnhub forbids sharing.
- **Its attribution rule is concrete,** and the engine's credit meets it.
- **The key never rides in a URL,** since it goes in a header from any page.

**The cost of the choice.** Users must pay Twelve Data to present its prices lawfully, which the editors say plainly where its key is added. Before the editors ship, ask Twelve Data (api@twelvedata.com, the address its attribution guide gives) whether a series kept in a deck's data file is stored data under Grow and Venture.

## Consequences

- **+** One record, one plan shape, one credit, and one date rule serve three objects, and the haiku is one more writing task. Every client asks and patches through the same Rust.
- **+** A deck with a map, a forecast, or a price chart draws the same offline, in a single file, in the PDF, and in video, on every painter, held by the goldens. Fetching again is an ordinary, undoable edit in the history.
- **+** A map and a non-commercial forecast cost the user nothing to start: Geoapify's Free plan and Open-Meteo's free API. A haiku costs about a cent with a mid-priced model, and less than a tenth of a cent with the cheapest the providers list (their published prices, read 2026-10-15).
- **−** A map's picture is the provider's, not the theme's. A re-theme from light to dark leaves its style until Fetch Again; only the pin, its label, and the credit follow the theme.
- **−** A user needs a Geoapify key for a map, and Twelve Data's paid plan to present prices to others. Scaena cannot see a user's plan, so it says what each plan allows where the key is added.
- **−** Two credits show on a map from Geoapify's Free plan: its own in the picture's corner, and Scaena's. Only Scaena's is legible, linked, and read wherever the map is shown.
- **−** The MCP server reaches the network for the first time. Its tools say what they send where, and an agent's host may have to allow it.
- **−** What is typed goes to the service: an address to Geoapify, a place to Open-Meteo, a ticker to Twelve Data, and the talk's claims and notes to the model's provider. Each sheet says so before it asks, and coordinates skip a geocoder.
- **−** The editor's module may run out of room (decision 9). Tables are handed over before any budget moves.
- **−** It takes five deck formats (0.31–0.35, and 0.36 if the optional closed axis lands) and four theme formats (0.21–0.24). Each deck bump rewrites the decks that name the format, and each theme bump the themes.
- **−** Terms move. Stadia's change on 2026-11-09, Open-Meteo's take effect when posted, and Twelve Data's attribution guide dates from January 2026. Each adapter rereads its service's terms when it is touched, and recorded answers catch an answer whose shape drifts.

## Alternatives

- **A map drawn by the engine** from OpenStreetMap's features (Nominatim for the place; OpenFreeMap's vector tiles), kept in the bundle and colored by the theme. Not chosen (Decided). It would also have leaned on two donated services with no SLA, and on Nominatim's one request a second for the whole app.
- **Apple's MapKit snapshots** on the Mac and the iPad, which need no key. Apple's terms forbid storing Map Data beyond a temporary basis and publishing it (decision 10).
- **MapTiler's or Mapbox's static maps.** MapTiler forbids keeping static images; Mapbox's terms on keeping one were not found, and its unauthorized answer sends no CORS header.
- **A server of Scaena's,** to pool and cache requests or to host Open-Meteo's server. Invariant 7.
- **A key of the publisher's in every client.** It would be shared with anyone who looks, against the services' terms and invariant 7.
- **Fetching at render,** a live source refreshed as a deck plays. That breaks determinism, fails offline, and leaves nothing for the PDF; SPEC §16 Q4 stays open.
- **The forecast as the deck's own nodes** (decision 19), and **the credit as an ordinary caption node.** A person could delete the caption or move it off the slide, and every client would have to keep it with its object.
- **Shipping CMUdict whole:** 363,489 bytes gzipped, more than twice the editor's room. **Trusting the model's count:** wrong on about half the sentences.

## Sources

Each page was opened on 2026-10-15, through the session's proxy. A page that could not be opened, and a claim it would have carried, is marked unverified.

**Maps**

| Source | What it gave |
|---|---|
| https://www.geoapify.com/static-maps-api/ | Static maps may be "freely" cached, stored, and redistributed with OpenStreetMap's credit; the print version may be used in commercial products at no extra cost; 4,096 × 4,096 px; scale factor up to 2 |
| https://apidocs.geoapify.com/docs/maps/static/ | Its parameters (`apiKey`, `style`, `width`, `height`, `center`, `zoom` 1–20, `scaleFactor`, `format`); the credit it asks (Geoapify on the Free plan, OpenStreetMap, OpenMapTiles), written in the picture's corner by default; print credit beside the map or in acknowledgments; cost, 1 credit plus a credit for each four 256-px tiles |
| https://www.geoapify.com/pricing/ | Free: 3,000 credits a day, up to 5 requests a second; paid plans from $59 a month; commercial use on the Free plan in production, with the credits |
| https://www.geoapify.com/terms-and-conditions/ | OpenStreetMap's credit always; Geoapify's on the Free plan; Free's commercial use "with some limitations" in production, by contact. Nothing on keeping maps |
| https://apidocs.geoapify.com/docs/geocoding/forward-geocoding/ | Its geocoder: `text`, `lang`, `limit`, filters; one credit a request; a key may be held to origins |
| https://www.geoapify.com/geocoding-api/ | Its geocoding results may be stored, with their sources credited |
| https://maps.geoapify.com/v1/staticmap (unauthorized, with an Origin header) | 401 with `Access-Control-Allow-Origin: *`; the same from `api.geoapify.com/v1/geocode/search`. CORS on an authorized answer is unverified |
| https://docs.stadiamaps.com/static-maps/ | The `static_cacheable` endpoint (kept for digital use while subscribed; printed up to 5,000 copies); sizes 256–4,096 points with `@2x`; zoom 0–18; `manual_attribution`; the key in a query or an `Authorization: Stadia-Auth` header |
| https://stadiamaps.com/pricing/ | Free (no commercial use, no static maps); Starter $20 a month, 1,000,000 credits, 3¢ per 1,000 more; Standard $80; Professional $250; a static map 20 credits, a cacheable one 2,000, a forward geocode 20; geocoding results stored for good from Standard |
| https://stadiamaps.com/terms-of-service/ | Terms in force from 2026-03-18: the Cacheable Static Maps Addendum (print up to 5,000 copies an image; digital use while a paid subscription is kept); no sharing of keys or subscriptions; the free tier for non-commercial use or evaluation |
| https://stadiamaps.com/terms-of-service/versions/2026-11-09/ | From 2026-11-09: Digital Embeddable Usage, permanently embedded and redistributed up to 5,000 copies; geocoding results stored while on Standard or above; "Commercial Use" defined |
| https://docs.stadiamaps.com/attribution/ | "© Stadia Maps © OpenMapTiles © OpenStreetMap", linked; in print and video, the URLs printed |
| https://tiles.stadiamaps.com/static_cacheable/alidade_smooth.png (unauthorized, and its preflight) | 401 and 204 with `Access-Control-Allow-Origin: *` and `Access-Control-Allow-Headers: Stadia-Auth,Content-Type` |
| https://developer.apple.com/support/terms/apple-developer-program-license-agreement/ | The Apple Maps Service includes "related tools for capturing map content (e.g., MapSnapshotter)"; Attachment 6, §2.3 and §2.5 |
| https://developer.apple.com/tutorials/data/documentation/mapkit/mkmapsnapshotter.json | MKMapSnapshotter's documentation: it captures map tiles and imagery into an image, and grants no right to keep one |
| https://www.maptiler.com/terms/ and https://www.maptiler.com/cloud/terms/ | Read by the research pass: no storing or redistributing map content as a static image; internal prints only; no modifying map content |
| https://docs.mapbox.com/api/maps/static-images.md, https://www.mapbox.com/legal/tos, and https://www.mapbox.com/legal/product-terms | 1,280 px a side, attribution required. No clause on keeping a static image found in its terms of service or product terms: unverified either way. Its unauthorized answer sent no CORS header (research pass) |
| https://www.openstreetmap.org/copyright and https://osmfoundation.org/wiki/Licence/Attribution_Guidelines | Read by the research pass: credit OpenStreetMap and make the ODbL clear; in PDFs and print, the URL openstreetmap.org/copyright printed |
| https://operations.osmfoundation.org/policies/nominatim/, https://openfreemap.org/, and https://openfreemap.org/tos/ | Read by the research pass, for the vector map not chosen: Nominatim's one request a second for a whole app; OpenFreeMap's free tiles with no SLA |

**Weather**

| Source | What it gave |
|---|---|
| https://open-meteo.com/en/terms | Non-commercial use only on the free API, with its examples of both kinds of use; under 10,000 calls a day, 5,000 an hour, 600 a minute, 300,000 a month; CC BY 4.0; misuse blocked without notice; no sharing of account or login information; the contact address |
| https://open-meteo.com/en/licence | "Weather data by Open-Meteo.com", linked, next to the data; GeoNames for places; AGPLv3 server |
| https://open-meteo.com/en/pricing | The `customer-` hosts and `apikey`; how calls are counted; a 99.9% uptime target. The page's own text shows no prices |
| https://merchant-ui-api.stripe.com/pricing-table/prctbl_1MJg0iLNZMSyEuRUvpoeTHOb | The price table the pricing page draws (research pass): Standard $29 a month or $319 a year, Professional $99 or $1,099, Enterprise by contact |
| https://open-meteo.com/en/docs and https://open-meteo.com/en/docs/geocoding-api | Its variables, `timezone=auto`, units, errors, the WMO codes, and the geocoder (research pass) |
| https://geocoding-api.open-meteo.com/v1/search and https://api.open-meteo.com/v1/forecast | Live answers (research pass): `Access-Control-Allow-Origin: *` on all four hosts; postal codes found in the United States and much of Europe, none in Canada, the United Kingdom, Australia, Japan, or India. When a chance of rain comes back null is unverified |
| https://openweathermap.org/price, https://openweathermap.org/api/one-call-3, and https://openweathermap.org/faq | One Call by the call; its attribution with its logo (research pass) |
| https://docs.api.met.no/doc/TermsOfService and https://docs.api.met.no/doc/License | MET Norway's identification, traffic, and licences (research pass) |

**Market data**

| Source | What it gave |
|---|---|
| https://twelvedata.com/pricing | Basic: "for testing, evaluation, or development purposes only… cannot be displayed to users, shared externally, or used in production"; Grow from $29, view-only internal display; Pro from $99 |
| https://twelvedata.com/pricing-business | Venture, $149 a month: external display data access; Enterprise from $1,099; white labeling at the top |
| https://twelvedata.com/terms, https://twelvedata.com/docs, and https://support.twelvedata.com/en/articles/12647398-attribution-guidelines-for-using-twelve-data | Read by the research pass: display only as a tier allows; `/time_series` and its errors; "Data provided by Twelve Data", linked. Its help article on the depth of history answered 404: unverified |
| https://www.alphavantage.co/premium/, https://www.alphavantage.co/documentation/, and https://www.alphavantage.co/terms_of_service/ | Read by the research pass: 25 requests a day; intraday premium; personal, non-commercial use |
| https://massive.com/pricing, https://massive.com/business, and https://massive.com/knowledge-base/article/which-plan-do-i-need-to-show-massive-data-in-my-app | Read by the research pass: Stocks Basic free; Stocks Business $2,499 a month for anyone else to see |
| https://finnhub.io/pricing, https://finnhub.io/docs/api/stock-candles, and https://finnhub.io/terms-of-service | Read by the research pass: candles premium; personal use; no sharing |

**The haiku**

| Source | What it gave |
|---|---|
| https://arxiv.org/html/2404.02456v1 | PhonologyBench: the best model counted a sentence's syllables exactly 55.3% of the time, a human 90.0% |
| https://www.hsa-haiku.org/hsa-definitions.html and https://www.hsa-haiku.org/hsa-definitions-2004.html | The Haiku Society of America: its definition counts no syllables; most English haiku are seventeen syllables or fewer |
| https://raw.githubusercontent.com/cmusphinx/cmudict/master/README and its LICENSE | CMUdict: use for any purpose unrestricted, its origin acknowledged; BSD-style |
| https://unpkg.com/syllable@5.0.1/readme.md | words/syllable 5.0.1, MIT |
| https://storage.googleapis.com/books/ngrams/books/datasetsv3.html | Google Books Ngram: CC BY 3.0 |
| https://en.wikipedia.org/wiki/Mora_(linguistics) | How kana are counted as morae |
| https://developers.openai.com/api/docs/pricing and https://ai.google.dev/gemini-api/docs/pricing | Published prices a million tokens, behind the haiku's cost under Consequences: about $2 in and $10 out for a mid-priced model, and as little as $0.10 in and $0.50 out |
| https://www.anthropic.com/legal/aup and https://developer.apple.com/support/terms/acceptable-use-requirements-for-the-foundation-models-framework | No content falsely attributed; nothing in Apple's list bars a short poem |

The research pass's own measurements, not a source's, are unverified as such: how often the counter is right (97% of running words by its rules, 99.9% with the table, against CMUdict), the bytes of its table and the icons' paths, and the estimates in decision 9.
