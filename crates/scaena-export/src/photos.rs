//! `scaena_core::jpeg` decodes as zune-jpeg does (ADR-0017): each JPEG in `tests/fixtures/jpeg`,
//! as stored, is the same bytes from both. zune-jpeg is what `image` reads JPEGs with, and on
//! this machine it takes whichever of its paths the processor allows, so the test also says its
//! paths agree with its portable one on these files.

use std::path::Path;

#[test]
fn photos_decode_as_zune_jpeg_does() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/jpeg");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    names.sort();
    let mut read = 0;
    for path in names {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).unwrap();
        let ours = scaena_core::jpeg::decode_stored(&bytes);
        if name == "cmyk.jpg" {
            assert!(ours.unwrap_err().to_string().contains("CMYK"), "{name} is refused, saying why");
            continue;
        }
        let ours = ours.unwrap_or_else(|e| panic!("{name}: {e}"));
        let theirs = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).unwrap().to_rgba8();
        assert_eq!((ours.width, ours.height), theirs.dimensions(), "{name}: its size");
        let differ: Vec<usize> = (0..ours.rgba.len()).filter(|&i| ours.rgba[i] != theirs.as_raw()[i]).collect();
        if let Some(&first) = differ.first() {
            let (px, w) = (first / 4, ours.width as usize);
            panic!(
                "{name}: {} of {} bytes differ, the first at ({}, {}): {:?} where zune-jpeg has {:?}",
                differ.len(),
                ours.rgba.len(),
                px % w,
                px / w,
                &ours.rgba[px * 4..px * 4 + 4],
                &theirs.as_raw()[px * 4..px * 4 + 4]
            );
        }
        read += 1;
    }
    assert!(read >= 17, "read {read} JPEGs");
}
