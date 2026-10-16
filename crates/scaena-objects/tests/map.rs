//! Geoapify's map plan on recorded answers (`tests/fixtures/maps`, ADR-0030, PLAN O.1b): a place
//! found, several and a pick, none, its 401 and its 429 in its own words, coordinates that skip the
//! geocoder, a browser's plan with no User-Agent, and the key in no URL the plan writes.

use scaena_objects::map::{Ask, MapPlan, Show, Style, View};
use scaena_objects::{Answer, Get, Key, ObjectError, Ran, Transport, run};
use std::cell::RefCell;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/maps");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

/// Geoapify, scripted: its geocoder answers `places` (with `status`), its static map the recorded
/// picture. Each request is kept as Rust wrote it, and as it was sent with the key.
struct Scripted {
    places: (u16, Vec<u8>),
    picture: (u16, Vec<u8>),
    sent: RefCell<Vec<(Get, String)>>,
}

impl Scripted {
    fn new(places: (u16, &str), picture: (u16, &str)) -> Scripted {
        Scripted {
            places: (places.0, fixture(places.1)),
            picture: (picture.0, fixture(picture.1)),
            sent: RefCell::default(),
        }
    }
}

impl Transport for Scripted {
    fn keyed(&self, service: &str) -> bool {
        service == "geoapify"
    }
    fn send(&self, get: &Get) -> Result<Answer, String> {
        self.sent.borrow_mut().push((get.clone(), get.keyed_url("SECRET-KEY")));
        let ((status, body), kind) = if get.url.starts_with("https://api.geoapify.com/v1/geocode/search?") {
            (&self.places, "application/json")
        } else if get.url.starts_with("https://maps.geoapify.com/v1/staticmap?") {
            (&self.picture, "image/png")
        } else {
            return Err(format!("no such host: {}", get.url));
        };
        Ok(Answer { status: *status, kind: kind.into(), body: body.clone() })
    }
    fn now(&self) -> String {
        "2026-10-15T20:12:44Z".into()
    }
}

fn ask(place: &str) -> Ask {
    Ask {
        place: place.into(),
        known: None,
        center: None,
        view: View::Show(Show::Neighborhood),
        style: Style::Light,
        size: [800.0, 450.0],
        lang: Some("en-US".into()),
        pick: None,
        list: false,
        browser: false,
    }
}

#[test]
fn a_place_found_is_drawn_centered_on_it_with_the_credit_its_terms_ask() {
    let geo = Scripted::new((200, "places-one.json"), (200, "map.png"));
    let (mut plan, first) = MapPlan::new(ask("Ferry Building, San Francisco")).unwrap();
    assert_eq!(
        first.url,
        "https://api.geoapify.com/v1/geocode/search?text=Ferry%20Building%2C%20San%20Francisco&lang=en&limit=5&format=json"
    );
    assert_eq!(first.key, Some(Key::Query("apiKey".into())));
    let Ran::Done(map) = run(&mut plan, first, &geo).unwrap() else { panic!("one place, a map") };
    assert_eq!(map.png, fixture("map.png"));
    assert_eq!(map.place_name(), "Ferry Building");
    assert_eq!(map.pin, [0.5, 0.5], "centered on the place");
    let sent = geo.sent.borrow();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[1].0.url,
        "https://maps.geoapify.com/v1/staticmap?style=positron&width=800&height=450&scaleFactor=2&center=lonlat:-122.3937,37.7955&zoom=15.59&format=png"
    );
    // The key rides only in what is sent, never in what Rust writes.
    for (get, keyed) in sent.iter() {
        assert!(!get.url.contains("SECRET-KEY") && !get.url.contains("apiKey"), "{}", get.url);
        assert!(keyed.ends_with("&apiKey=SECRET-KEY"), "{keyed}");
        assert!(get.headers.iter().any(|(k, v)| k == "user-agent" && v.starts_with("Scaena/")));
    }
    let fetched = map.fetched("2026-10-15T20:12:44Z");
    assert!(scaena_core::fetched::problems(&serde_json::to_value(&fetched).unwrap()).is_empty());
    assert_eq!(
        scaena_core::fetched::said(&fetched.credit),
        "©\u{a0}OpenStreetMap contributors · openstreetmap.org/copyright · ©\u{a0}OpenMapTiles · Powered by Geoapify"
    );
    let m = fetched.map.unwrap();
    assert_eq!((m.style.as_str(), m.size, m.scale, m.zoom), ("positron", [800, 450], 2, 15.59));
}

#[test]
fn several_places_are_offered_and_one_picked_is_drawn_with_its_sources_credit() {
    let geo = Scripted::new((200, "places-several.json"), (200, "map.png"));
    let (mut plan, first) = MapPlan::new(ask("Springfield")).unwrap();
    let Ran::Choose(found) = run(&mut plan, first, &geo).unwrap() else { panic!("three places, a choice") };
    let said: Vec<&str> = found.iter().map(|f| f.address.as_str()).collect();
    assert_eq!(
        said,
        [
            "Springfield, IL, United States of America",
            "Springfield, MA, United States of America",
            "Springfield, MO, United States of America"
        ]
    );
    assert_eq!(geo.sent.borrow().len(), 1, "nothing drawn before one is picked");
    let (mut plan, first) = MapPlan::new(Ask { pick: Some(1), ..ask("Springfield") }).unwrap();
    let Ran::Done(map) = run(&mut plan, first, &geo).unwrap() else { panic!("picked, a map") };
    assert_eq!((map.map.place.as_ref().unwrap().lat, map.map.place.as_ref().unwrap().lon), (42.1015, -72.5898));
    let last = map.credit.last().unwrap();
    assert_eq!(
        (last.text.as_str(), last.link.as_deref()),
        ("Who's On First", Some("https://www.whosonfirst.org/docs/licenses/"))
    );
    // Listed, even one place is offered, and nothing drawn.
    let one = Scripted::new((200, "places-one.json"), (200, "map.png"));
    let (mut plan, first) = MapPlan::new(Ask { list: true, ..ask("Ferry Building") }).unwrap();
    assert!(matches!(run(&mut plan, first, &one).unwrap(), Ran::Choose(f) if f.len() == 1));
    assert_eq!(one.sent.borrow().len(), 1);
    let (mut plan, first) = MapPlan::new(Ask { pick: Some(3), ..ask("Springfield") }).unwrap();
    let err = run(&mut plan, first, &geo).unwrap_err();
    assert!(err.to_string().contains("pick one of 1 to 3"), "{err}");
}

#[test]
fn none_found_and_each_refusal_are_said_in_geoapifys_words() {
    let geo = Scripted::new((200, "places-none.json"), (200, "map.png"));
    let (mut plan, first) = MapPlan::new(ask("Nowhere at all, Atlantis")).unwrap();
    let err = run(&mut plan, first, &geo).unwrap_err();
    assert!(err.to_string().contains("found no place called “Nowhere at all, Atlantis”"), "{err}");
    for (status, body, words) in [
        (401, "unauthorized.json", "Unauthorized: Invalid apiKey"),
        (429, "too-many.json", "Too Many Requests: You have exceeded the credits limit for today"),
    ] {
        let geo = Scripted::new((status, body), (200, "map.png"));
        let (mut plan, first) = MapPlan::new(ask("Ferry Building")).unwrap();
        let err = run(&mut plan, first, &geo).unwrap_err();
        assert_eq!(err, ObjectError::Refused { service: "Geoapify".into(), status, words: words.into() });
        assert_eq!(err.to_string(), format!("Geoapify refused it: {words}"));
        assert!(!err.to_string().contains("SECRET-KEY"));
    }
    // A picture refused too.
    let geo = Scripted::new((200, "places-one.json"), (401, "unauthorized.json"));
    let (mut plan, first) = MapPlan::new(ask("Ferry Building")).unwrap();
    assert!(matches!(run(&mut plan, first, &geo), Err(ObjectError::Refused { status: 401, .. })));
}

#[test]
fn coordinates_skip_the_geocoder_and_a_browser_sets_no_user_agent() {
    let geo = Scripted::new((200, "places-one.json"), (200, "map.png"));
    let asked = Ask { browser: true, view: View::Zoom(12.0), style: Style::Dark, ..ask("37.7955, -122.3937") };
    let (mut plan, first) = MapPlan::new(asked).unwrap();
    assert!(first.url.starts_with("https://maps.geoapify.com/v1/staticmap?style=dark-matter&"), "{}", first.url);
    assert!(first.url.contains("&zoom=12&"), "{}", first.url);
    assert!(first.headers.iter().all(|(k, _)| k != "user-agent"), "{:?}", first.headers);
    let Ran::Done(map) = run(&mut plan, first, &geo).unwrap() else { panic!("a map") };
    assert_eq!(geo.sent.borrow().len(), 1, "no geocoder");
    assert_eq!(map.place_name(), "37.7955, -122.3937", "what was typed names it");
    assert!(MapPlan::new(ask("  ")).is_err());
}
