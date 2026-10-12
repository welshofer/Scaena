//! Narrative drift (Vzestup's `drift.ts`): the slides evaluated whose place in the deck now puts
//! them in another beat than the one they were written for, a place read as the allocator that
//! made the deck spread its slides. A slide moved, or moved round, may be worth a look.

use super::SlideForHealth;
use crate::beats;
use serde::{Deserialize, Serialize};

/// A slide that drifted: the beat it was written for, the beat its place puts it in, and its
/// place among the slides evaluated, from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftedSlide {
    pub id: String,
    /// Its title, or "(untitled)".
    pub title: String,
    pub original_beat: usize,
    pub current_beat: usize,
    pub position: usize,
}

/// The beats the slides were written for, as many as the last of them names, and the slides
/// that drifted from theirs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftReport {
    pub total_beats: usize,
    pub drifted: Vec<DriftedSlide>,
}

/// The drift of `slides` (`computeNarrativeDrift`), read with the beat allocator of the version
/// the deck was stamped with ([`beats::beat_for`]); a deck made before decks were stamped reads
/// as version 1 made it. Slides hidden or left out of evaluation have no place; a slide written
/// for no beat drifts from none.
pub fn detect(slides: &[SlideForHealth], allocator: Option<u32>) -> DriftReport {
    let allocator = allocator.unwrap_or(1);
    let visible: Vec<&SlideForHealth> = slides.iter().filter(|s| s.eligible()).collect();
    let total_beats = visible.iter().filter_map(|s| s.beat()).max().map_or(0, |b| b + 1);
    if total_beats == 0 {
        return DriftReport::default();
    }
    let drifted = (visible.iter().enumerate())
        .filter_map(|(i, s)| {
            let original_beat = s.beat()?;
            let current_beat = beats::beat_for(i, visible.len(), total_beats, allocator);
            (current_beat != original_beat).then(|| DriftedSlide {
                id: s.id.clone(),
                title: if s.title.is_empty() { "(untitled)".into() } else { s.title.clone() },
                original_beat,
                current_beat,
                position: i + 1,
            })
        })
        .collect();
    DriftReport { total_beats, drifted }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::NarrativeMeta;

    /// A slide with no words but its title, written for `beat` (`makeSlide`).
    fn at(id: &str, title: &str, beat: Option<usize>) -> SlideForHealth {
        let narrative_meta = beat.map(|b| NarrativeMeta { beat_index: Some(b), ..NarrativeMeta::default() });
        SlideForHealth { id: id.into(), title: title.into(), narrative_meta, ..SlideForHealth::default() }
    }

    fn tagged(beats: &[(&str, usize)]) -> Vec<SlideForHealth> {
        beats.iter().map(|&(id, beat)| at(id, "", Some(beat))).collect()
    }

    fn drifted(id: &str, title: &str, original_beat: usize, current_beat: usize, position: usize) -> DriftedSlide {
        DriftedSlide { id: id.into(), title: title.into(), original_beat, current_beat, position }
    }

    #[test]
    fn no_slide_written_for_a_beat_is_no_beat_at_all() {
        assert_eq!(detect(&[], None), DriftReport { total_beats: 0, drifted: Vec::new() });
        let untagged = [at("a", "A", None), at("b", "B", None)];
        assert_eq!(detect(&untagged, None), DriftReport::default());
    }

    #[test]
    fn slides_in_their_beats_have_not_drifted() {
        let report = detect(&tagged(&[("1", 0), ("2", 0), ("3", 1), ("4", 1), ("5", 2), ("6", 2)]), None);
        assert_eq!((report.total_beats, report.drifted.len()), (3, 0));
    }

    #[test]
    fn slides_moved_across_a_boundary_have_drifted() {
        // Two each of three beats; the third and the fifth swapped: each is evidence.
        let slides = [
            at("1", "", Some(0)),
            at("2", "", Some(0)),
            at("moved-back", "Moved back", Some(2)),
            at("4", "", Some(1)),
            at("moved-forward", "Moved forward", Some(1)),
            at("6", "", Some(2)),
        ];
        let report = detect(&slides, None);
        assert_eq!(report.total_beats, 3);
        assert_eq!(
            report.drifted,
            [drifted("moved-back", "Moved back", 2, 1, 3), drifted("moved-forward", "Moved forward", 1, 2, 5)]
        );
        // A slide added at the end for the first beat: the rest stay; it overflows into the last.
        let mut slides = tagged(&[("1", 0), ("2", 0), ("3", 1), ("4", 1)]);
        slides.push(at("stray", "Stray", Some(0)));
        let report = detect(&slides, None);
        assert_eq!((report.total_beats, report.drifted), (2, vec![drifted("stray", "Stray", 0, 1, 5)]));
    }

    #[test]
    fn hidden_and_left_out_slides_have_no_place() {
        let mut slides = vec![SlideForHealth { hidden: true, ..at("h", "", Some(5)) }];
        slides.extend(tagged(&[("1", 0), ("2", 0), ("3", 1), ("4", 1)]));
        assert_eq!(detect(&slides, None), DriftReport { total_beats: 2, drifted: Vec::new() });
        let mut slides = vec![SlideForHealth { exclude_from_evaluation: true, ..at("qa", "", Some(9)) }];
        slides.extend(tagged(&[("1", 0), ("2", 1)]));
        assert_eq!(detect(&slides, None), DriftReport { total_beats: 2, drifted: Vec::new() });
        // Its place among those evaluated, from 1: the hidden one does not count.
        let mut slides = vec![SlideForHealth { hidden: true, ..at("h", "", Some(0)) }];
        slides.extend(tagged(&[("1", 0), ("2", 0), ("moved", 2), ("4", 1), ("5", 1), ("6", 2)]));
        let report = detect(&slides, None);
        assert_eq!(report.drifted.iter().find(|d| d.id == "moved").map(|d| d.position), Some(3));
    }

    #[test]
    fn a_slide_with_no_title_drifts_untitled() {
        let report = detect(&tagged(&[("1", 0), ("2", 0), ("moved", 2), ("4", 1), ("5", 1), ("6", 2)]), None);
        let entry = report.drifted.iter().find(|d| d.id == "moved").expect("it drifted");
        assert_eq!(entry.title, "(untitled)");
    }

    #[test]
    fn fewer_slides_than_beats_take_one_beat_each() {
        let report = detect(&[at("first", "", Some(0)), at("last", "End", Some(3))], None);
        assert_eq!((report.total_beats, report.drifted), (4, vec![drifted("last", "End", 3, 1, 2)]));
    }

    #[test]
    fn a_slide_written_for_no_beat_still_holds_its_place() {
        let report = detect(&[at("1", "", Some(0)), at("2", "", None), at("3", "", Some(1))], None);
        assert_eq!(report.total_beats, 2);
        assert!(!report.drifted.iter().any(|d| d.id == "2"));
    }

    /// The beats a freshly made deck of `n` slides over `b` beats carries, by allocator v2.
    fn planned(n: usize, b: usize) -> Vec<SlideForHealth> {
        let beats = (crate::beats::allocate(n, b).into_iter().enumerate()).flat_map(|(beat, k)| vec![beat; k]);
        beats.enumerate().map(|(i, beat)| at(&format!("s{i}"), "", Some(beat))).collect()
    }

    #[test]
    fn the_allocator_a_deck_was_made_by_reads_it() {
        // 13 over 7, the case v2 was made for: it reads back whole, and v1 reads it as drifted.
        let slides = planned(13, 7);
        let v2 = detect(&slides, Some(2));
        assert_eq!((v2.total_beats, v2.drifted.len()), (7, 0));
        let v1 = detect(&slides, Some(1));
        assert_eq!(v1.total_beats, 7);
        assert!(!v1.drifted.is_empty());
        // A deck stamped with none reads as v1.
        let slides = planned(20, 6);
        assert_eq!(detect(&slides, None), detect(&slides, Some(1)));
        // Where the slides divide evenly, both agree.
        let slides = planned(6, 3);
        assert!(detect(&slides, Some(2)).drifted.is_empty() && detect(&slides, Some(1)).drifted.is_empty());
    }
}
