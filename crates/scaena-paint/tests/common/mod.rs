//! Shared by the painter tests: the torture goldens and the bundle fonts and images they
//! name.

use scaena_core::displaylist::DisplayList;
use scaena_paint::Assets;

pub const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
pub const GOLDEN: &str = "../../tests/golden/torture";

/// Every golden display list, by state, in name order.
pub fn goldens() -> Vec<(String, DisplayList)> {
    let mut out: Vec<(String, DisplayList)> = std::fs::read_dir(GOLDEN)
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            let state = name.strip_suffix(".dl.json")?.to_string();
            let json = std::fs::read_to_string(format!("{GOLDEN}/{name}")).unwrap();
            Some((state, DisplayList::from_json(&json).unwrap()))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The bundle fonts the display lists name, and the bundle's images by content id.
pub fn assets(dls: &[(String, DisplayList)]) -> Assets {
    use sha2::{Digest, Sha256};
    let mut store = Assets::new();
    let ids: std::collections::BTreeSet<&str> =
        dls.iter().flat_map(|(_, dl)| dl.fonts.iter().map(|f| f.id.as_str())).collect();
    for id in ids {
        store.insert_font(id, std::fs::read(format!("{BUNDLE}/{id}")).unwrap());
    }
    for entry in std::fs::read_dir(format!("{BUNDLE}/assets")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "png") {
            let bytes = std::fs::read(&path).unwrap();
            let hex: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            store.insert_image(&format!("sha256:{hex}"), &bytes).unwrap();
        }
    }
    store
}
