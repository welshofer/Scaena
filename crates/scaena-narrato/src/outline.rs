//! The outline before the slides (narrative FUNC-1): once the arc is chosen and before the slides
//! are paid for, the model is asked for the deck's big idea and one claim for each beat of the arc
//! at this length, as the plan spreads the slides over them. The person reads it, changes a claim,
//! moves, removes, or adds a beat, and the slides are then written to the outline they settled,
//! which Scaena's own block carries into the slides' prompt after [`crate::prompt::SLOT_SHAPES`]:
//! Narrato's text stands byte for byte (ADR-0026). A client that wants slides at once asks for no
//! outline, and nothing here is called.

use crate::arcs::NarrativeArc;
use crate::{Brief, beats, containment, js};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Room for the outline's answer, thinking included.
pub const OUTLINE_MAX_TOKENS: u32 = 16_000;

/// How much of the sources the outline sees: enough to ground each claim, short of the slides'
/// whole.
const SOURCE_UNITS: usize = 6_000;

/// The deck's outline: its big idea, and its beats in the order the slides take them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outline {
    /// The one sentence the audience should remember.
    pub big_idea: String,
    pub beats: Vec<Beat>,
}

/// One beat of the outline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Beat {
    /// Its name: the arc's, or the person's own for a beat they added.
    pub name: String,
    /// The one sentence its slides must leave the audience with.
    pub claim: String,
    /// The slides it takes, as the plan spreads them.
    pub slides: usize,
}

/// The rider the outline is asked under.
pub const OUTLINE_SYSTEM: &str = "You are planning a presentation before its slides are written. Output ONLY a JSON object \u{2014} no markdown, no code fence, no commentary \u{2014} with two keys: \"bigIdea\" (string: the one sentence the audience should remember when the talk is over) and \"beats\" (an array with one object for each beat given, in the order given, each with \"beat\" (number, as given) and \"claim\" (string: the one sentence this beat's slides must leave the audience believing, specific to this presentation and its material, never the beat's generic name)). Write each sentence plainly, in the language the presentation is in, at most 25 words.";

/// The beats the outline is asked for: those `plan` gives slides at this length, each by its
/// number in the arc, its name, and its slides.
fn planned<'a>(arc: &'a NarrativeArc, plan: &[(usize, usize, usize)]) -> Vec<(usize, &'a str, usize)> {
    plan.iter()
        .filter(|&&(_, from, to)| to >= from)
        .map(|&(beat, from, to)| (beat + 1, arc.beats[beat].as_str(), to - from + 1))
        .collect()
}

/// The prompt that asks for `brief`'s outline along `arc`, chosen for `reasoning`, its slides
/// spread as `plan` spreads them.
pub fn prompt(brief: &Brief, arc: &NarrativeArc, reasoning: &str, plan: &[(usize, usize, usize)]) -> String {
    let mut out = format!(
        "A presentation is to be written about \"{}\", for {}, to {}, {} long.",
        brief.topic, brief.audience, brief.goal, brief.length
    );
    if let Some(outcomes) = brief.desired_outcomes.as_deref().map(js::trim).filter(|s| !s.is_empty()) {
        out.push_str(&format!(" It should achieve: {outcomes}."));
    }
    if let Some(language) = brief.language.as_deref().map(js::trim).filter(|l| !l.is_empty()) {
        out.push_str(&format!(" It is in {language}: write the big idea and every claim in {language}."));
    }
    out.push_str(&format!(
        "\n\nIt follows the {} arc: {} Why this arc: {}\n\nIts beats, with the slides each takes at this length:\n",
        arc.arc,
        arc.premise,
        js::trim(reasoning)
    ));
    for (number, name, slides) in planned(arc, plan) {
        let count = if slides == 1 { "1 slide".to_string() } else { format!("{slides} slides") };
        out.push_str(&format!("  Beat {number} \"{name}\" \u{2192} {count}\n"));
    }
    out.push_str(
        "\nGive the big idea, then one claim for each beat above, in order: what its slides must leave the audience believing, said as the audience would repeat it. Each claim follows from the one before, and the last lands the big idea.",
    );
    if let Some(content) = brief.source_content.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!(
            "\n\nGround each claim in this material, which the slides will be written from:\n{}",
            containment::wrap_source_excerpt(&js::first_units(content, SOURCE_UNITS))
        ));
    }
    out
}

/// The answer's contract, as Gemini takes it (the others derive theirs from it).
pub fn schema_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "bigIdea": { "type": "STRING" },
            "beats": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": { "beat": { "type": "NUMBER" }, "claim": { "type": "STRING" } },
                    "required": ["beat", "claim"]
                }
            }
        },
        "required": ["bigIdea", "beats"]
    })
}

/// The outline an answer gives: each beat `plan` gives slides, in the arc's order, with the claim
/// the answer wrote for it, or none where it wrote none (which the person sees empty, and
/// [`settled`] passes over).
pub fn read(text: &str, arc: &NarrativeArc, plan: &[(usize, usize, usize)]) -> Result<Outline, String> {
    let start = text.find('{').ok_or("the answer holds no JSON object")?;
    let end = text.rfind('}').ok_or("the answer holds no JSON object")?;
    let value: Value =
        serde_json::from_str(&text[start..=end]).map_err(|e| format!("the answer is not the JSON asked for: {e}"))?;
    let big_idea = value.get("bigIdea").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
    let answered = value.get("beats").and_then(Value::as_array).ok_or("the answer has no `beats`")?;
    let claim = |number: usize| {
        answered
            .iter()
            .find(|b| b.get("beat").and_then(Value::as_f64).is_some_and(|n| n == number as f64))
            .and_then(|b| b.get("claim").and_then(Value::as_str))
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let beats: Vec<Beat> = planned(arc, plan)
        .into_iter()
        .map(|(number, name, slides)| Beat { name: name.to_string(), claim: claim(number), slides })
        .collect();
    if beats.is_empty() {
        return Err("the arc has no beat at this length".into());
    }
    if beats.iter().all(|b| b.claim.is_empty()) {
        return Err("the answer gives no beat its claim".into());
    }
    Ok(Outline { big_idea, beats })
}

/// `outline` as the person settled it, ready for the slides: each claim and name trimmed, a beat
/// with no claim passed over, a beat with no name called by its place, and `target` slides spread
/// over the beats that stand as the plan spreads them. None left is refused.
pub fn settled(outline: &Outline, target: usize) -> Result<Outline, String> {
    let mut beats: Vec<Beat> = (outline.beats.iter())
        .filter(|b| !b.claim.trim().is_empty())
        .map(|b| Beat { name: b.name.trim().to_string(), claim: b.claim.trim().to_string(), slides: 0 })
        .collect();
    if beats.is_empty() {
        return Err("an outline keeps at least one beat with a claim".into());
    }
    for (i, beat) in beats.iter_mut().enumerate() {
        if beat.name.is_empty() {
            beat.name = format!("Beat {}", i + 1);
        }
    }
    for (beat, count) in beats.iter_mut().zip(beats::allocate(target.max(1), outline.beats.len().max(1))) {
        beat.slides = count;
    }
    // Fewer beats than before leave the spread short: spread again over those that stand.
    let n = beats.len();
    for (beat, count) in beats.iter_mut().zip(beats::allocate(target.max(n), n)) {
        beat.slides = count;
    }
    Ok(Outline { big_idea: outline.big_idea.trim().to_string(), beats })
}

/// Scaena's block for the slides' prompt, after [`crate::prompt::SLOT_SHAPES`]: the outline the
/// person settled, which the slides follow where it and Narrato's own plan differ.
pub fn block(outline: &Outline) -> String {
    let mut out = String::from(
        "\n**Outline (Scaena)** \u{2014} the user has read and settled this outline; the slides follow it, and where it differs from the Structure and the Beat Plan above, this outline wins.",
    );
    if !outline.big_idea.is_empty() {
        out.push_str(&format!(" The big idea, which every slide serves: \"{}\".", outline.big_idea));
    }
    out.push_str(
        "\nIts beats, in this order, each with the slides it takes and the one claim its slides must leave the audience with:\n",
    );
    let mut cursor = 1;
    for (i, beat) in outline.beats.iter().enumerate() {
        let range = match beat.slides {
            0 => "(no slides at this length)".to_string(),
            1 => format!("slide {cursor}"),
            n => format!("slides {cursor}\u{2013}{}", cursor + n - 1),
        };
        cursor += beat.slides;
        out.push_str(&format!("  Beat {} \"{}\" \u{2192} {range}: {}\n", i + 1, beat.name, beat.claim));
    }
    out.push_str(
        "Write the slides in this order with these counts, `beatIndex` the number of a slide's beat here less one and `beatName` that beat's name, and every slide carrying its beat's claim.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arcs;

    fn brief() -> Brief {
        Brief {
            topic: "Fifty-two miles of trail".into(),
            audience: "Donors".into(),
            goal: "Inspire".into(),
            length: "5-10 slides".into(),
            desired_outcomes: Some("Fund next summer".into()),
            ..Brief::default()
        }
    }

    #[test]
    fn the_prompt_names_each_beat_with_its_slides() {
        let arc = arcs::arc("Problem\u{2013}Solution");
        let plan = beats::plan(arc, 5);
        let p = prompt(&brief(), arc, "The donors know the problem.", &plan);
        assert!(p.starts_with("A presentation is to be written about \"Fifty-two miles of trail\", for Donors, to Inspire, 5-10 slides long. It should achieve: Fund next summer."));
        assert!(p.contains("It follows the Problem\u{2013}Solution arc:"));
        assert!(p.contains("Why this arc: The donors know the problem."));
        assert!(p.contains("  Beat 1 \"Problem: Define core challenge and stakes\" \u{2192} 1 slide\n"));
        // Five slides over six beats: the sixth has none, and is not asked for.
        assert!(!p.contains("Beat 6"), "{p}");
        assert!(!p.contains("Ground each claim"));
        let grounded =
            Brief { source_content: Some("=== SOURCE: notes.md ===\nPort calls doubled.".into()), ..brief() };
        let g = prompt(&grounded, arc, "", &plan);
        assert!(g.contains("Ground each claim in this material") && g.contains("Port calls doubled."));
        let czech = Brief { language: Some("Czech".into()), ..brief() };
        assert!(prompt(&czech, arc, "", &plan).contains("write the big idea and every claim in Czech"));
    }

    #[test]
    fn an_answer_reads_as_the_outline_and_a_missing_claim_stays_empty() {
        let arc = arcs::arc("Problem\u{2013}Solution");
        let plan = beats::plan(arc, 5);
        let answer = r#"Here: {"bigIdea": " Trails are kept by the people who walk them. ", "beats": [
            {"beat": 1, "claim": "The storm took the trail."},
            {"beat": 3, "claim": "Volunteers rebuilt it."},
            {"beat": 2, "claim": "  "}
        ]}"#;
        let outline = read(answer, arc, &plan).unwrap();
        assert_eq!(outline.big_idea, "Trails are kept by the people who walk them.");
        assert_eq!(outline.beats.len(), 5);
        assert_eq!(outline.beats[0].claim, "The storm took the trail.");
        assert_eq!(outline.beats[1].claim, "");
        assert_eq!(outline.beats[2].claim, "Volunteers rebuilt it.");
        assert_eq!(outline.beats[0].name, "Problem: Define core challenge and stakes");
        assert!(outline.beats.iter().all(|b| b.slides == 1));
        assert!(read("no json", arc, &plan).is_err());
        assert!(read(r#"{"bigIdea": "x", "beats": []}"#, arc, &plan).unwrap_err().contains("no beat its claim"));
    }

    #[test]
    fn a_settled_outline_keeps_the_order_drops_blank_claims_and_spreads_the_slides() {
        let outline = Outline {
            big_idea: " One idea ".into(),
            beats: vec![
                Beat { name: "Solution".into(), claim: "We rebuilt it.".into(), slides: 1 },
                Beat { name: "Problem".into(), claim: "   ".into(), slides: 1 },
                Beat { name: "".into(), claim: "Give again.".into(), slides: 1 },
                Beat { name: "Proof".into(), claim: "Ninety miles walked.".into(), slides: 9 },
            ],
        };
        let settled = settled(&outline, 8).unwrap();
        assert_eq!(settled.big_idea, "One idea");
        let names: Vec<&str> = settled.beats.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Solution", "Beat 2", "Proof"]);
        let slides: Vec<usize> = settled.beats.iter().map(|b| b.slides).collect();
        assert_eq!(slides, [2, 3, 3], "eight slides over three beats, the rest from the last back");
        assert!(super::settled(&Outline::default(), 8).is_err());
    }

    #[test]
    fn the_block_tells_the_slides_the_outline() {
        let outline = Outline {
            big_idea: "Trails are kept by those who walk them.".into(),
            beats: vec![
                Beat { name: "Problem".into(), claim: "The storm took the trail.".into(), slides: 2 },
                Beat { name: "Solution".into(), claim: "Volunteers rebuilt it.".into(), slides: 1 },
                Beat { name: "Ask".into(), claim: "Give again.".into(), slides: 0 },
            ],
        };
        let b = block(&outline);
        assert!(b.starts_with("\n**Outline (Scaena)**"));
        assert!(b.contains("The big idea, which every slide serves: \"Trails are kept by those who walk them.\"."));
        assert!(b.contains("  Beat 1 \"Problem\" \u{2192} slides 1\u{2013}2: The storm took the trail.\n"));
        assert!(b.contains("  Beat 2 \"Solution\" \u{2192} slide 3: Volunteers rebuilt it.\n"));
        assert!(b.contains("  Beat 3 \"Ask\" \u{2192} (no slides at this length): Give again.\n"));
        assert!(b.ends_with("every slide carrying its beat's claim.\n"));
    }
}
