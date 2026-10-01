//! Shared by the painter tests: the torture goldens and the bundle fonts they name.

use scaena_core::displaylist::DisplayList;
use scaena_paint::FontStore;

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

/// The bundle fonts the display lists name.
pub fn fonts(dls: &[(String, DisplayList)]) -> FontStore {
    let mut store = FontStore::new();
    let ids: std::collections::BTreeSet<&str> =
        dls.iter().flat_map(|(_, dl)| dl.fonts.iter().map(|f| f.id.as_str())).collect();
    for id in ids {
        store.insert(id, std::fs::read(format!("{BUNDLE}/{id}")).unwrap());
    }
    store
}
