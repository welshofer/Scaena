//! A map put on a slide (ADR-0030, PLAN O.1b): in the slide's empty place for a picture, in an
//! image, or at a box; one patch, refused where the deck would be invalid; and fetched again into
//! the same image, both pictures kept in the bundle.

use scaena_core::fetched::{CreditRun, Fetched, LatLon, MapAsked, Place};
use scaena_ops::maps::{Aim, Fetch, aiming, place, placing, refreshing};
use std::path::{Path, PathBuf};

const EXAMPLES: &str = "../../docs/examples";

/// The revenue example in a bundle of its own, with what it names.
fn bundle(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("maps-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let deck: serde_json::Value =
        serde_json::from_slice(&std::fs::read(format!("{EXAMPLES}/revenue.deck.json")).unwrap()).unwrap();
    let mut files: Vec<String> =
        deck["fonts"].as_array().unwrap().iter().map(|f| f["file"].as_str().unwrap().into()).collect();
    files.push(deck["theme"].as_str().unwrap().into());
    for source in deck["data"].as_object().unwrap().values() {
        if let Some(path) = source["source"].as_str() {
            files.push(path.into());
        }
    }
    for f in files {
        let to = dir.join(&f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(format!("{EXAMPLES}/{f}"), to).unwrap();
    }
    std::fs::write(dir.join("deck.json"), serde_json::to_vec_pretty(&deck).unwrap()).unwrap();
    dir
}

fn picture(n: u8) -> Vec<u8> {
    let mut png = std::fs::read("../../tests/fixtures/maps/map.png").unwrap();
    // Another picture: the same pixels with a byte of its own after the end, still a PNG.
    png.push(n);
    png
}

fn fetch(png: Vec<u8>, zoom: f64) -> Fetch {
    let place = Place { name: Some("Ferry Building".into()), lat: 37.7955, lon: -122.3937 };
    let fetched = Fetched {
        from: "geoapify".into(),
        at: "2026-10-15T20:12:44Z".into(),
        asked: "Ferry Building, San Francisco".into(),
        credit: vec![CreditRun {
            text: "© OpenStreetMap contributors".into(),
            link: Some("https://www.openstreetmap.org/copyright".into()),
        }],
        license: Some("ODbL-1.0".into()),
        map: Some(MapAsked {
            place: Some(place),
            center: LatLon { lat: 37.7955, lon: -122.3937 },
            zoom,
            style: "positron".into(),
            size: [800, 450],
            scale: 2,
        }),
        terms: None,
        kept: false,
        edited: false,
        forecast: None,
    };
    Fetch { png, fetched, pin: [0.5, 0.5], label: None }
}

#[test]
fn a_map_fills_the_slides_empty_place_for_a_picture_in_one_patch() {
    let dir = bundle("slot");
    let b = scaena_ops::open(&dir).unwrap();
    let target = aiming(&b, &Aim::default()).unwrap();
    assert_eq!((target.state.as_str(), target.slot.as_deref(), target.style.as_str()), ("intro", Some("art"), "dark"));
    assert_eq!(target.lang.as_deref(), Some("en-US"));
    let placed = place(&b, &target, &fetch(picture(1), 15.0)).unwrap();
    assert!(placed.patched.applied, "{:#?}", placed.patched);
    assert_eq!(placed.node, "map");
    assert!(placed.src.starts_with("assets/") && placed.src.ends_with(".png"));
    assert_eq!(placed.said, "Map of Ferry Building from Geoapify added");
    let b = scaena_ops::open(&dir).unwrap();
    assert!(b.read(&placed.src).is_ok(), "the picture joined the bundle");
    let node = &b.deck.nodes["map"].props;
    assert_eq!(node["at"], serde_json::json!({ "in": "art" }));
    assert_eq!(node["pin"]["label"], "Ferry Building");
    assert_eq!(node["fetched"]["from"], "geoapify");
    // It stays on its slide: the next slide hides it.
    let snaps = scaena_core::resolve_states(&b.deck).unwrap();
    assert!(snaps[0].nodes.contains_key("map") && !snaps[1].nodes.contains_key("map"));
    assert!(
        scaena_ops::lint::lint(&b)
            .unwrap()
            .findings
            .iter()
            .all(|f| f.node.as_deref() != Some("map") || f.severity != scaena_core::Severity::Error)
    );
}

#[test]
fn a_map_goes_in_an_image_or_at_a_box_and_is_refused_where_the_deck_would_be_invalid() {
    let dir = bundle("box");
    let b = scaena_ops::open(&dir).unwrap();
    // On a slide with no empty place for a picture, it needs a box or an image.
    let err = aiming(&b, &Aim { state: Some("revenue".into()), ..Aim::default() }).unwrap_err();
    assert!(err.message.contains("no empty place for a picture"), "{err}");
    let aim = Aim { state: Some("revenue".into()), at: Some([100.0, 100.0, 400.0, 225.0]), into: None };
    let target = aiming(&b, &aim).unwrap();
    let placed = place(&b, &target, &fetch(picture(2), 15.0)).unwrap();
    assert!(placed.patched.applied);
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(b.deck.nodes["map"].props["at"], serde_json::json!({ "rect": [100.0, 100.0, 400.0, 225.0] }));
    // Into the image: its picture and record replaced, its place kept, one patch.
    let into = aiming(&b, &Aim { state: Some("revenue".into()), into: Some("map".into()), at: None }).unwrap();
    assert_eq!(into.rect, [100.0, 100.0, 400.0, 225.0]);
    let mut other = fetch(picture(3), 16.0);
    other.label = Some(String::new());
    let (put, write) = placing(&b, &into, &other).unwrap();
    assert!(put.patched.applied && write.is_some());
    assert_eq!(put.said, "Map of Ferry Building from Geoapify put in the picture");
    let ops: Vec<String> =
        put.patched.patch.iter().map(|op| serde_json::to_value(op).unwrap()["path"].as_str().unwrap().into()).collect();
    assert!(ops.iter().all(|p| p.starts_with("/nodes/map/") || p.starts_with("/states/")), "{ops:?}");
    // Not an image, not a node: said, nothing written.
    let err = aiming(&b, &Aim { state: Some("revenue".into()), into: Some("title".into()), at: None }).unwrap_err();
    assert!(err.message.contains("is no image"), "{err}");
    // A record the deck refuses (a service this build does not know) refuses the patch.
    let mut bad = fetch(picture(4), 15.0);
    bad.fetched.from = "mapquest".into();
    let (refused, write) = placing(&b, &into, &bad).unwrap();
    assert!(!refused.patched.applied && write.is_none());
    assert!(refused.patched.added.iter().any(|f| f.code == "E106"), "{:#?}", refused.patched.added);
}

#[test]
fn fetching_again_asks_for_the_same_view_and_keeps_both_pictures() {
    let dir = bundle("refresh");
    let b = scaena_ops::open(&dir).unwrap();
    let first = place(&b, &aiming(&b, &Aim::default()).unwrap(), &fetch(picture(5), 15.0)).unwrap();
    let b = scaena_ops::open(&dir).unwrap();
    let again = refreshing(&b, Some("intro"), "map", None, None).unwrap();
    assert_eq!(
        (again.zoom, again.style.as_str(), again.asked.as_str()),
        (15.0, "positron", "Ferry Building, San Francisco")
    );
    assert_eq!(again.place.name.as_deref(), Some("Ferry Building"));
    assert_eq!(again.target.node.as_deref(), Some("map"));
    let zoomed = refreshing(&b, Some("intro"), "map", Some(16.0), Some(LatLon { lat: 37.8, lon: -122.4 })).unwrap();
    assert_eq!((zoomed.zoom, zoomed.center), (16.0, LatLon { lat: 37.8, lon: -122.4 }));
    let second = place(&b, &again.target, &fetch(picture(6), 16.0)).unwrap();
    assert!(second.patched.applied);
    assert_ne!(first.src, second.src);
    let b = scaena_ops::open(&dir).unwrap();
    assert!(b.read(&first.src).is_ok() && b.read(&second.src).is_ok(), "both pictures stay until one is taken out");
    assert_eq!(b.deck.nodes["map"].props["src"], second.src.as_str());
    assert_eq!(b.deck.nodes["map"].props["fetched"]["map"]["zoom"], 16.0);
    let err = refreshing(&b, Some("intro"), "title", None, None).unwrap_err();
    assert!(err.message.contains("was not fetched"), "{err}");
}
