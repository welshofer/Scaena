//! A theme's contact sheet (PLAN T.0e, ADR-0031 decision 8): `scaena theme --sheet` draws every
//! layout of a theme, started as Add Slide starts it, and each start's display-list digest is kept
//! in `tests/golden/themes/<slug>.digests.json`, so a layout that draws otherwise is caught.
//!
//! On a mismatch the test writes the digests it found to `tests/golden/themes/actual/`. After the
//! change is reviewed (each start's sheet, `scaena theme --sheet`), `just bless` writes them:
//! `SCAENA_BLESS=1 cargo test -p scaena-cli --test sheet`.

use std::path::Path;
use std::process::Command;

const GOLDEN: &str = "../../tests/golden/themes";

fn scaena(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

/// The themes that ship, by slug, as the registry lists them.
fn slugs() -> Vec<String> {
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read("../../docs/themes/registry.json").unwrap()).unwrap();
    registry["themes"].as_array().unwrap().iter().map(|t| t["slug"].as_str().unwrap().to_string()).collect()
}

/// Each start's digest, by its layout, in the theme's order.
fn digests(sheet: &scaena_ops::sheet::Sheet) -> serde_json::Map<String, serde_json::Value> {
    sheet.starts.iter().map(|s| (s.layout.clone(), s.digest.clone().into())).collect()
}

/// Every shipped theme's starts draw as their digests say (PLAN T.0e).
#[test]
fn every_layout_of_every_shipped_theme_starts_as_its_digest_says() {
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let actual = Path::new(GOLDEN).join("actual");
    let mut drifted = Vec::new();
    for slug in slugs() {
        let sheet = scaena_ops::sheet::sheet(&slug, false).unwrap();
        let found = serde_json::to_string_pretty(&digests(&sheet)).unwrap() + "\n";
        let golden = Path::new(GOLDEN).join(format!("{slug}.digests.json"));
        if bless {
            std::fs::write(&golden, &found).unwrap();
            continue;
        }
        let kept = std::fs::read_to_string(&golden).unwrap_or_default();
        if kept != found {
            let was: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&kept).unwrap_or_default();
            let now = digests(&sheet);
            let changed: Vec<&String> = now
                .keys()
                .chain(was.keys())
                .filter(|k| was.get(*k) != now.get(*k))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            std::fs::create_dir_all(&actual).unwrap();
            std::fs::write(actual.join(format!("{slug}.digests.json")), &found).unwrap();
            drifted.push(format!("{slug}: {changed:?}"));
        }
    }
    if bless {
        let _ = std::fs::remove_dir_all(&actual);
    }
    assert!(
        drifted.is_empty(),
        "starts draw otherwise than their digests say:\n{}\nThe digests found are in tests/golden/themes/actual/. \
         Review each theme's sheet (`scaena theme --sheet SLUG --out FILE`), then bless with SCAENA_BLESS=1.",
        drifted.join("\n")
    );
}

/// The sheet of Dusk: one PNG of every layout, each start's digest said, and the same digests on
/// a second run.
#[test]
fn the_sheet_of_dusk_draws_every_layout_and_says_each_digest() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dusk-sheet.png");
    let _ = std::fs::remove_file(&out);
    let run = scaena(&["--json", "theme", "--sheet", "dusk", "--out", out.to_str().unwrap()]);
    assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
    let said: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let png = std::fs::read(&out).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let theme: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
    let layouts: Vec<&String> = theme["layouts"].as_object().unwrap().keys().collect();
    let starts = said["starts"].as_array().unwrap();
    assert_eq!(starts.iter().map(|s| s["layout"].as_str().unwrap()).collect::<Vec<_>>(), layouts);
    let [w, h] = [said["size"][0].as_u64().unwrap(), said["size"][1].as_u64().unwrap()];
    assert!(w > 1000 && h > w, "{w} × {h}");

    let again = scaena_ops::sheet::sheet("dusk", false).unwrap();
    let digests: Vec<&str> = starts.iter().map(|s| s["digest"].as_str().unwrap()).collect();
    assert_eq!(again.starts.iter().map(|s| s.digest.as_str()).collect::<Vec<_>>(), digests);
}

/// A layout drawn otherwise is caught: Dusk with its title slide's title moved a row down gives
/// the title start another digest, and every other start its own.
#[test]
fn a_changed_layout_is_caught_as_drift() {
    let text = include_str!("../../../docs/examples/themes/dusk.theme.json");
    let mut theme: serde_json::Value = serde_json::from_str(text).unwrap();
    let title = &mut theme["layouts"]["title"]["slots"]["title"]["row"];
    let row: Vec<u64> = title.as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    *title = serde_json::json!([row[0] + 1, row[1] + 1]);
    let font = |file: &str| -> Result<Vec<u8>, scaena_ops::OpsError> {
        Ok(scaena_ops::shipped::font(file).expect("a font that ships").to_vec())
    };
    let sheet = |text: String| scaena_ops::sheet::sheet_of(Path::new("dusk.theme.json"), text, &font, false).unwrap();
    let (was, now) = (digests(&sheet(text.to_string())), digests(&sheet(theme.to_string())));
    let changed: Vec<&String> = now.keys().filter(|k| was[*k] != now[*k]).collect();
    assert_eq!(changed, ["title"]);
}
