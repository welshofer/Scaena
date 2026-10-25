//! A picture an agent is handed put on a slide (lift-up 10-24b FUNC-5): in the slide's empty place
//! for a picture, at a box, or in an image; named by its content in `assets/`; one patch, kept to
//! its slide; one too large refused, for the client to make smaller first.

use scaena_ops::pictures::{Picture, add, adding};
use std::path::{Path, PathBuf};

const EXAMPLES: &str = "../../docs/examples";

/// The revenue example in a bundle of its own, with what it names.
fn bundle(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("pictures-{test}"));
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

fn png() -> Vec<u8> {
    std::fs::read("../../tests/fixtures/maps/map.png").unwrap()
}

fn jpeg() -> Vec<u8> {
    std::fs::read("../../tests/fixtures/jpeg/tiny.jpg").unwrap()
}

#[test]
fn a_picture_fills_the_slides_empty_place_for_one_described_and_kept_to_its_slide() {
    let dir = bundle("slot");
    let b = scaena_ops::open(&dir).unwrap();
    let asked = Picture { alt: Some(" The company's logo ".into()), ..Picture::default() };
    let added = add(&b, &png(), &asked).unwrap();
    assert!(added.patched.applied, "{:#?}", added.patched);
    assert_eq!((added.node.as_str(), added.said.as_str()), ("picture", "Picture added"));
    assert!(added.src.starts_with("assets/") && added.src.ends_with(".png"), "{}", added.src);
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(b.read(&added.src).unwrap(), png(), "the picture joined the bundle as it was");
    let node = &b.deck.nodes["picture"].props;
    assert_eq!(node["at"], serde_json::json!({ "in": "art" }));
    assert_eq!(node["alt"], "The company's logo");
    // It stays on its slide: the next slide hides it.
    let snaps = scaena_core::resolve_states(&b.deck).unwrap();
    assert!(snaps[0].nodes.contains_key("picture") && !snaps[1].nodes.contains_key("picture"));
}

#[test]
fn a_picture_goes_at_a_box_or_in_an_image_and_one_too_large_is_refused() {
    let dir = bundle("box");
    let b = scaena_ops::open(&dir).unwrap();
    // On a slide with no empty place for a picture, it needs a box or an image.
    let revenue = Picture { state: Some("revenue".into()), ..Picture::default() };
    let err = add(&b, &png(), &revenue).unwrap_err();
    assert!(err.message.contains("no empty place for a picture"), "{err}");
    let boxed = Picture { at: Some([100.0, 100.0, 400.0, 225.0]), ..revenue.clone() };
    let added = add(&b, &png(), &boxed).unwrap();
    assert!(added.patched.applied);
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(b.deck.nodes["picture"].props["at"], serde_json::json!({ "rect": [100.0, 100.0, 400.0, 225.0] }));
    // Into the image: a JPEG in place of the PNG, its place kept, one patch; a dry run writes nothing.
    let into = Picture { into: Some("picture".into()), ..revenue.clone() };
    let (put, write) = adding(&b, &jpeg(), &into).unwrap();
    assert!(put.patched.applied && write.is_some());
    assert_eq!((put.node.as_str(), put.said.as_str()), ("picture", "Picture put in the image"));
    assert!(put.src.ends_with(".jpg"), "{}", put.src);
    let paths: Vec<String> =
        put.patched.patch.iter().map(|op| serde_json::to_value(op).unwrap()["path"].as_str().unwrap().into()).collect();
    assert!(paths.iter().all(|p| p.starts_with("/nodes/picture/") || p.starts_with("/states/")), "{paths:?}");
    // Not an image, not a node, not a picture: said, nothing written.
    let err = add(&b, &png(), &Picture { into: Some("title".into()), ..revenue.clone() }).unwrap_err();
    assert!(err.message.contains("is no image"), "{err}");
    let err = add(&b, b"not a picture", &boxed).unwrap_err();
    assert!(err.message.starts_with("the picture:"), "{err}");
    // Past 8192 px a side: refused here, for the client to make it smaller first.
    let huge = scaena_paint::Raster { width: 8193, height: 1, rgba: vec![255; 8193 * 4] }.to_png_fast().unwrap();
    let err = add(&b, &huge, &boxed).unwrap_err();
    assert!(err.message.contains("8193 × 1 px") && err.message.contains("make it smaller first"), "{err}");
}
