//! The shipped themes' walkthroughs (PLAN 2.140, ADR-0029): on each theme's card, every kind of
//! token a line out of focus draws is set back to 3:1 or more, and set back far enough to show:
//! keywords and plain words at least twice as faint by their contrast, every kind at least 1.5×.

use scaena_core::code::Class;
use scaena_core::palette::{SET_BACK, ratio};
use scaena_engine::theme::Theme;

const THEMES: [&str; 4] = [
    "../../docs/examples/themes/dusk.theme.json",
    "../../docs/examples/authorability/themes/daybreak.theme.json",
    "../../docs/examples/themes/ember.theme.json",
    "../../docs/examples/themes/salon.theme.json",
];

#[test]
fn every_shipped_theme_sets_back_the_lines_out_of_focus_far_enough_to_show() {
    for path in THEMES {
        let theme = Theme::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(theme.code_dim(), 0.55, "{path}");
        let [r, g, b, _] = theme.code_card().unwrap().fill;
        let card = [r, g, b];
        let role = theme.text_role(theme.code_role().unwrap_or("body")).unwrap();
        let looks = theme.code_looks(card, &role).unwrap();
        let dimmed = theme.dimmed_looks(&looks, card);
        let rgb = |c: [u8; 4]| [c[0], c[1], c[2]];
        for (k, (look, set)) in looks.iter().zip(&dimmed).enumerate() {
            let (was, now) = (ratio(rgb(look.color), card), ratio(rgb(set.color), card));
            assert!(now >= SET_BACK, "{path}: kind {k} reads {now:.2}:1 set back");
            assert_eq!((look.weight, look.italic), (set.weight, set.italic), "{path}: kind {k} keeps its face");
            // A color is whole bytes, so a kind held at 4.5:1 sets back to the byte just over 3:1, a
            // hundredth or two above it: 1.5× to within 1%.
            let least = if k == Class::Plain as usize || k == Class::Keyword as usize { 2.0 } else { 1.5 };
            assert!(was / now >= least * 0.99, "{path}: kind {k} {was:.2}:1 set back only to {now:.2}:1");
        }
        // The light themes band the lines in focus, and the code in focus reads on the band.
        let band = theme.code_band(card).unwrap();
        let light = path.contains("daybreak") || path.contains("salon");
        assert_eq!(band.is_some(), light, "{path}");
        if let Some(band) = band {
            for look in theme.code_looks(band, &role).unwrap() {
                assert!(ratio(rgb(look.color), band) >= 4.5, "{path}: {look:?} on its band");
            }
        }
    }
}
