# Geoapify's answers, as recorded for the map's plan (ADR-0030, PLAN O.1b)

No test makes a live call. These are the answers `scaena_objects::map` reads, in the shapes Geoapify's documentation gives for its geocoder (`format=json`) and its errors, written out by hand on 2026-10-15 from those pages: no key was used to make them, and the places are rounded.

- `places-one.json`: `Ferry Building, San Francisco`, one place, from OpenStreetMap.
- `places-several.json`: `Springfield`, three cities, one of them from Who's On First, whose credit joins the map's.
- `places-none.json`: a place it finds nothing for.
- `unauthorized.json`: its 401, for a key it does not take.
- `too-many.json`: its 429, for a day's credits spent.
- `map.png`: the static map's answer, a stand-in: the torture deck's map card (`scripts/build_torture_images.py`).

A recorded picture asked with Geoapify's own marker, which would hold the pin's place to its marker within half a pixel, waits on a key of Jay's.
