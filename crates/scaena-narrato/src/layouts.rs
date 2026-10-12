//! What each slide's layout name comes to, and the deck's layouts varied (Narrato's
//! `layoutSuggestionMap`, `resolveLayoutSpecId`, and `layoutVariety`): a structural layout, and
//! the rich layout the name canonically names, if any; then a deterministic pass that opens on a
//! title, closes on a closing, caps section dividers and novelty formats, and breaks up runs and
//! a layout's dominance where a slide's content fits another. Each slide's identity, the rich id
//! or else the structural one, is what Scaena maps onto a theme's layouts.

use crate::answer::Slide;
use regex_lite::Regex;
use std::sync::OnceLock;

/// A slide's layout as Narrato keeps it: its structural layout and its rich one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub layout: &'static str,
    pub spec: Option<&'static str>,
}

impl Layout {
    /// What the slide is drawn as: its rich layout, else its structural one.
    pub fn identity(&self) -> &'static str {
        self.spec.unwrap_or(self.layout)
    }
}

/// A layout name, case-folded, to its structural layout; any other is `body`.
const BY_SUGGESTION: [(&str, &str); 144] = [
    ("title", "title"),
    ("section", "section"),
    ("section header", "section"),
    ("agenda", "agenda"),
    ("table of contents", "agenda"),
    ("closing", "closing"),
    ("thank you", "closing"),
    ("summary", "closing"),
    ("body", "body"),
    ("bullet list", "body"),
    ("bullets", "body"),
    ("numbered", "numbered"),
    ("numbered list", "numbered"),
    ("ordered list", "numbered"),
    ("quote", "quote"),
    ("pull quote", "quote"),
    ("statement", "statement"),
    ("hero statement", "statement"),
    ("definition", "definition"),
    ("metric", "metric"),
    ("big number", "metric"),
    ("scorecard", "scorecard"),
    ("kpis", "scorecard"),
    ("dashboard", "scorecard"),
    ("bar chart", "bar-chart"),
    ("bar-chart", "bar-chart"),
    ("line chart", "line-chart"),
    ("line-chart", "line-chart"),
    ("trend", "line-chart"),
    ("chart / data", "bar-chart"),
    ("chart/data", "bar-chart"),
    ("two-column", "two-column"),
    ("two column", "two-column"),
    ("comparison", "two-column"),
    ("before / after", "before-after"),
    ("before/after", "before-after"),
    ("before-after", "before-after"),
    ("pros / cons", "pros-cons"),
    ("pros/cons", "pros-cons"),
    ("pros-cons", "pros-cons"),
    ("quadrant", "quadrant"),
    ("matrix", "quadrant"),
    ("2x2", "quadrant"),
    ("swot", "quadrant"),
    ("raci matrix", "body"),
    ("raci", "body"),
    ("raci chart", "body"),
    ("okrs", "body"),
    ("okr", "body"),
    ("objectives and key results", "body"),
    ("kpi grid", "scorecard"),
    ("kpi-grid", "scorecard"),
    ("kpi dashboard", "scorecard"),
    ("kpi tiles", "scorecard"),
    ("swot analysis", "quadrant"),
    ("vs", "two-column"),
    ("versus", "two-column"),
    ("head-to-head", "two-column"),
    ("head to head", "two-column"),
    ("scenarios", "framework"),
    ("scenario planning", "framework"),
    ("best case worst case", "framework"),
    ("three horizons", "framework"),
    ("3 horizons", "framework"),
    ("horizons", "framework"),
    ("team", "gallery"),
    ("our team", "gallery"),
    ("meet the team", "gallery"),
    ("leadership", "gallery"),
    ("testimonials", "quote"),
    ("customer quotes", "quote"),
    ("social proof", "quote"),
    ("faq", "two-column"),
    ("faqs", "two-column"),
    ("frequently asked questions", "two-column"),
    ("q&a", "section"),
    ("q & a", "section"),
    ("q and a", "section"),
    ("qa", "section"),
    ("questions and discussion", "section"),
    ("checklist", "numbered"),
    ("readiness checklist", "numbered"),
    ("launch checklist", "numbered"),
    ("ranking", "numbered"),
    ("top 10", "numbered"),
    ("top ten", "numbered"),
    ("leaderboard", "numbered"),
    ("ranked list", "numbered"),
    ("next steps", "numbered"),
    ("next-steps", "numbered"),
    ("action items", "numbered"),
    ("action plan", "numbered"),
    ("sales funnel", "framework"),
    ("conversion funnel", "framework"),
    ("road map", "timeline"),
    ("product roadmap", "timeline"),
    ("key milestones", "timeline"),
    ("2\u{d7}2 matrix", "quadrant"),
    ("2x2 matrix", "quadrant"),
    ("how it works", "steps"),
    ("pipeline", "steps"),
    ("stack", "framework"),
    ("the ask", "callout"),
    ("ask", "callout"),
    ("bio", "image-text"),
    ("glossary", "two-column"),
    ("playbook", "numbered"),
    ("recipe", "numbered"),
    ("thesis", "statement"),
    ("manifesto", "statement"),
    ("headline", "statement"),
    ("memo", "body"),
    ("letter", "body"),
    ("open letter", "body"),
    ("receipt", "body"),
    ("steps", "steps"),
    ("process", "steps"),
    ("process / steps", "steps"),
    ("process/steps", "steps"),
    ("timeline", "timeline"),
    ("roadmap", "timeline"),
    ("milestones", "timeline"),
    ("journey", "journey"),
    ("user journey", "journey"),
    ("customer journey", "journey"),
    ("image", "image"),
    ("image + caption", "image"),
    ("image+caption", "image"),
    ("full image", "image"),
    ("image-text", "image-text"),
    ("image + text", "image-text"),
    ("image and text", "image-text"),
    ("split", "image-text"),
    ("gallery", "gallery"),
    ("grid", "gallery"),
    ("case studies", "gallery"),
    ("framework", "framework"),
    ("pyramid", "framework"),
    ("funnel", "framework"),
    ("venn", "framework"),
    ("radial", "framework"),
    ("callout", "callout"),
    ("annotated diagram", "callout"),
    ("anatomy", "callout"),
];

/// The rich layouts a name may canonically name: each id, its name, the name Narrato's catalog
/// gives it, and what models have been seen to call it instead.
const SPECS: [(&str, &str, Option<&str>, &[&str]); 37] = [
    ("roadmap", "Roadmap", Some("Roadmap"), &["road map", "product roadmap"]),
    ("timeline", "Timeline", Some("Timeline"), &["chronology", "history timeline"]),
    (
        "matrix",
        "2\u{d7}2 matrix",
        Some("2\u{d7}2 Matrix"),
        &["2x2 matrix", "effort impact matrix", "prioritization matrix"],
    ),
    ("okrs", "OKRs", Some("OKRs"), &["okr", "objectives and key results"]),
    ("raci", "RACI", Some("RACI Matrix"), &["raci chart"]),
    ("swot", "SWOT", Some("SWOT"), &["swot analysis"]),
    ("pros-cons", "Pros & cons", Some("Pros / Cons"), &["advantages and disadvantages", "tradeoffs"]),
    ("quote", "Pull-quote", Some("Quote"), &["pull quote"]),
    ("thesis", "Thesis", Some("Thesis"), &["central argument", "core claim"]),
    ("letter", "Letter", Some("Letter"), &["open letter"]),
    ("memo", "Memo", Some("Memo"), &["internal memo"]),
    ("receipt", "Receipt", Some("Receipt"), &["itemized costs", "the bill"]),
    ("next-steps", "Next steps", Some("Next Steps"), &["action items", "action plan"]),
    ("milestones", "Milestones", Some("Milestones"), &["key milestones"]),
    ("3horizons", "Three horizons", Some("Three Horizons"), &["3 horizons", "horizons"]),
    ("funnel", "Funnel", Some("Funnel"), &["sales funnel", "conversion funnel"]),
    ("scenarios", "Scenarios", Some("Scenarios"), &["scenario planning", "best case worst case"]),
    ("vs", "Head-to-head", Some("VS"), &["versus", "head-to-head", "head to head"]),
    ("metric", "Hero metric", Some("Big Number"), &["hero metric", "north star metric"]),
    ("kpi-grid", "KPI grid", Some("KPI Grid"), &["kpi dashboard", "kpi tiles"]),
    ("ranking", "Ranking", Some("Ranking"), &["top 10", "top ten", "leaderboard", "ranked list"]),
    ("headline", "Headline", Some("Headline"), &["announcement", "big news"]),
    ("manifesto", "Manifesto", Some("Manifesto"), &["principles", "credo", "what we believe"]),
    ("team", "Team", Some("Team"), &["our team", "meet the team", "leadership"]),
    ("bio", "Bio", Some("Bio"), &["profile", "founder bio"]),
    ("testimonials", "Testimonials", Some("Testimonials"), &["customer quotes", "social proof"]),
    ("how-it-works", "How it works", Some("How It Works"), &["mechanism", "how does it work"]),
    ("pipeline", "Pipeline", Some("Pipeline"), &["workflow pipeline", "deal pipeline"]),
    ("stack", "Stack", Some("Stack"), &["tech stack", "layers"]),
    ("faq", "FAQ", Some("FAQ"), &["faqs", "frequently asked questions"]),
    ("glossary", "Glossary", Some("Glossary"), &["definitions", "terminology"]),
    ("checklist", "Checklist", Some("Checklist"), &["readiness checklist", "launch checklist"]),
    ("ask", "The ask", Some("The Ask"), &["the ask", "our ask", "funding ask"]),
    ("qa", "Q & A", Some("Q&A"), &["q and a", "questions and discussion"]),
    ("thanks", "Thank you", None, &[]),
    ("chyron", "Chyron", None, &[]),
    ("recipe-card", "Recipe", Some("Playbook"), &["playbook", "recipe"]),
];

/// `name`'s structural layout (`layoutFromSuggestion`): an exact, case-folded lookup.
pub fn structural(name: &str) -> &'static str {
    let key = crate::js::trim(name).to_lowercase();
    BY_SUGGESTION.iter().find(|(k, _)| *k == key).map_or("body", |(_, v)| v)
}

/// A name as the rich-layout lookup reads it: lowercase, each run of anything but ASCII letters
/// and digits one space.
fn token(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut out = String::new();
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out.trim().to_string()
}

/// The rich layout `name` canonically names (`resolveLayoutSpecIdFromSuggestion`): by id, then by
/// name, then by Narrato's name, then by an alias, the first to claim a token keeping it.
pub fn spec(name: &str) -> Option<&'static str> {
    let wanted = token(name);
    if wanted.is_empty() {
        return None;
    }
    let ids = SPECS.iter().map(|s| (s.0, s.0));
    let names = SPECS.iter().map(|s| (s.1, s.0));
    let narrato = SPECS.iter().filter_map(|s| s.2.map(|n| (n, s.0)));
    let aliases = SPECS.iter().flat_map(|s| s.3.iter().map(move |a| (*a, s.0)));
    ids.chain(names).chain(narrato).chain(aliases).find(|(t, _)| token(t) == wanted).map(|(_, id)| id)
}

/// What a name maps to (`narratoSlidesToVzestup`).
pub fn of(name: &str) -> Layout {
    Layout { layout: structural(name), spec: spec(name) }
}

/// The rich layouts the variety pass may swap a slide to, with their structural layouts.
const RICH: [(&str, &str); 11] = [
    ("swot", "quadrant"),
    ("roadmap", "timeline"),
    ("okrs", "body"),
    ("funnel", "framework"),
    ("kpi-grid", "scorecard"),
    ("milestones", "timeline"),
    ("vs", "two-column"),
    ("ranking", "numbered"),
    ("next-steps", "numbered"),
    ("ask", "callout"),
    ("glossary", "two-column"),
];

fn rich(spec: &'static str) -> Layout {
    let layout = RICH.iter().find(|(s, _)| *s == spec).expect("a rich target").1;
    Layout { layout, spec: Some(spec) }
}

fn plain(layout: &'static str) -> Layout {
    Layout { layout, spec: None }
}

const NOVELTY: [&str; 5] = ["memo", "letter", "receipt", "headline", "manifesto"];
const SWAP_SOURCES: [&str; 7] = ["body", "numbered", "two-column", "statement", "scorecard", "steps", "timeline"];
const STRUCTURAL: [&str; 4] = ["title", "section", "closing", "agenda"];
const SECTION_CAP: usize = 3;

/// Why the pass changed a slide's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    BookendTitle,
    BookendClosing,
    SectionCap,
    NoveltyCap,
    RunBreak,
    DominanceCap,
}

/// One change: the slide, its identity before and after, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub index: usize,
    pub from: &'static str,
    pub to: &'static str,
    pub reason: Reason,
}

/// A slide as the pass reads it: its layout, its title, and its body, its bullets each a
/// `• ` line (or, on a title or a section, its subtitle).
#[derive(Debug, Clone, PartialEq)]
pub struct Varied {
    pub layout: Layout,
    pub title: String,
    pub body: String,
    pub chart: bool,
}

impl Varied {
    /// `slide` as Narrato's converter makes it.
    pub fn of(slide: &Slide) -> Varied {
        let layout = of(&slide.layout_suggestion);
        let body = if matches!(layout.layout, "title" | "section") {
            slide.subtitle.clone()
        } else {
            let lines: Vec<String> = (slide.key_bullets.iter())
                .map(|b| {
                    let b = crate::js::trim(b);
                    if b.starts_with('\u{2022}') { b.to_string() } else { format!("\u{2022} {b}") }
                })
                .collect();
            lines.join("\n")
        };
        Varied { layout, title: slide.slide_title.clone(), body, chart: slide.chart_data.is_some() }
    }
}

fn bullet_lines(body: &str) -> Vec<&str> {
    body.split('\n').map(str::trim).filter(|l| l.starts_with('\u{2022}')).collect()
}

struct Signals {
    swot: Regex,
    axis: Regex,
    sequence: Regex,
    funnel: Regex,
    okr: Regex,
    compare: Regex,
    vs: Regex,
    ranking: Regex,
    next_steps: Regex,
    ask: Regex,
    term: Regex,
    metric: Regex,
}

fn signals() -> &'static Signals {
    static SIGNALS: OnceLock<Signals> = OnceLock::new();
    SIGNALS.get_or_init(|| {
        let r = |p: &str| Regex::new(p).expect("a signal");
        Signals {
            swot: r(r"(?i)\bswot\b"),
            axis: r(r"(?i)\b(2\s*[x×]\s*2|quadrant|matrix|impact[/ ]effort)\b"),
            sequence: r(
                r"(?i)\b(phase|phases|step|steps|stage|stages|quarter|q[1-4]|roadmap|rollout|milestone|milestones|timeline|sequence)\b",
            ),
            funnel: r(r"(?i)\b(funnel|conversion|pipeline)\b"),
            okr: r(r"(?i)\b(okrs?|objectives? (and|&) key results|key results)\b"),
            compare: r(r"(?i)\b(vs\.?|versus|compared?|comparison)\b"),
            vs: r(r"(?i)\S\s+(?:vs\.?|versus)\s+\S"),
            ranking: r(r"(?i)\b(top\s?\d+|top ten|ranking|ranked|leaderboard|league table)\b"),
            next_steps: r(r"(?i)\b(next steps?|action items?|action plan|immediate actions?)\b"),
            ask: r(r"(?i)\b(the ask|we (?:ask|need|request|seek)|rais(?:e|ing)|approv(?:e|al)|invest(?:ment)?|funding)\b"),
            term: r(r"^•\s*([^—:–]{2,40})[—:–]\s*\S"),
            metric: r(r"(\d+(?:\.\d+)?\s*%|[$€£]\s?\d|\b\d{2,}\b)"),
        }
    })
}

/// The layouts a slide's content fits, matched to what it says before the generic ones.
fn candidates(s: &Varied, position: f64) -> (Vec<Layout>, Vec<Layout>) {
    let sig = signals();
    let bullets = bullet_lines(&s.body);
    let n = bullets.len();
    let metric = bullets.iter().filter(|b| sig.metric.is_match(b)).count();
    let title = s.title.as_str();
    let hay = format!("{title}\n{}", s.body);
    let (mut matched, mut generic) = (Vec::new(), Vec::new());
    if n == 4 && sig.swot.is_match(title) {
        matched.push(rich("swot"));
    }
    if n == 4 && !sig.swot.is_match(title) && sig.axis.is_match(title) {
        matched.push(plain("quadrant"));
    }
    if (3..=6).contains(&n) && sig.sequence.is_match(&hay) {
        matched.extend([rich("roadmap"), plain("steps"), rich("milestones")]);
    }
    if (3..=5).contains(&n) && sig.funnel.is_match(&hay) {
        matched.push(rich("funnel"));
    }
    if (2..=5).contains(&n) && sig.okr.is_match(&hay) {
        matched.push(rich("okrs"));
    }
    if n >= 3 && metric >= 3 {
        matched.push(if n > 4 { rich("kpi-grid") } else { plain("scorecard") });
    }
    if (3..=6).contains(&n) && sig.ranking.is_match(&hay) {
        matched.push(rich("ranking"));
    }
    if (2..=6).contains(&n) && sig.next_steps.is_match(&hay) {
        matched.push(rich("next-steps"));
    }
    if position >= 0.66 && (1..=4).contains(&n) && sig.ask.is_match(&hay) {
        matched.push(rich("ask"));
    }
    let glossary = n >= 4
        && bullets.iter().all(|b| {
            sig.term.captures(b).and_then(|c| c.get(1)).is_some_and(|t| t.as_str().trim().encode_utf16().count() <= 24)
        });
    if glossary
        && !sig.swot.is_match(title)
        && !sig.axis.is_match(title)
        && ![&sig.sequence, &sig.funnel, &sig.okr, &sig.ranking, &sig.next_steps].iter().any(|r| r.is_match(&hay))
    {
        matched.push(rich("glossary"));
    }
    let vs = n >= 2 && n.is_multiple_of(2) && sig.vs.is_match(title);
    if vs {
        matched.push(rich("vs"));
    }
    if !vs && n >= 4 && n.is_multiple_of(2) && sig.compare.is_match(title) {
        matched.push(plain("two-column"));
    }
    if s.layout.layout == "statement" && n <= 1 {
        matched.push(plain("callout"));
    }
    if (3..=5).contains(&n) {
        generic.extend([plain("numbered"), plain("two-column")]);
    }
    (matched, generic)
}

/// Narrato's variety pass (`enforceLayoutVariety`) over `slides`, in place: what it changed.
/// `bookends` false leaves the first and last slides as they are (an outline's).
pub fn vary(slides: &mut [Varied], bookends: bool) -> Vec<Change> {
    let mut changes = Vec::new();
    if slides.len() < 3 {
        return changes;
    }
    let mut swap = |slides: &mut [Varied], i: usize, to: Layout, reason| {
        let from = slides[i].layout.identity();
        changes.push(Change { index: i, from, to: to.identity(), reason });
        slides[i].layout = to;
    };
    let swappable = |s: &Varied| SWAP_SOURCES.contains(&s.layout.layout) && s.layout.spec.is_none() && !s.chart;

    if bookends {
        for (i, to, reason, limit) in
            [(0, "title", Reason::BookendTitle, 200), (slides.len() - 1, "closing", Reason::BookendClosing, 240)]
        {
            let s = &slides[i];
            if s.layout.layout == to {
                continue;
            }
            let bullets: Vec<String> = bullet_lines(&s.body).into_iter().map(str::to_string).collect();
            let fits = bullets.len() <= 1
                && s.body.encode_utf16().count() < limit
                && ["statement", "body", "callout", "section"].contains(&s.layout.layout);
            if fits && !s.chart {
                swap(slides, i, plain(to), reason);
                if bullets.len() == 1 {
                    slides[i].body =
                        bullets[0].trim_start_matches('\u{2022}').trim_start_matches(crate::js::is_space).to_string();
                }
            }
        }
    }

    let mut sections = 0;
    for i in 0..slides.len() {
        if slides[i].layout.layout != "section" {
            continue;
        }
        sections += 1;
        if sections <= SECTION_CAP {
            continue;
        }
        let one_liner = bullet_lines(&slides[i].body).len() <= 1 && slides[i].body.encode_utf16().count() < 160;
        swap(slides, i, plain(if one_liner { "statement" } else { "body" }), Reason::SectionCap);
    }

    let mut novelty = false;
    for i in 0..slides.len() {
        if slides[i].layout.spec.is_none_or(|s| !NOVELTY.contains(&s)) {
            continue;
        }
        if !novelty {
            novelty = true;
            continue;
        }
        let to = plain(slides[i].layout.layout);
        swap(slides, i, to, Reason::NoveltyCap);
    }

    let cap = 2usize.max(slides.len().div_ceil(4));
    let histogram = |slides: &[Varied]| {
        let mut h: Vec<(&'static str, usize)> = Vec::new();
        for s in slides.iter().filter(|s| !STRUCTURAL.contains(&s.layout.layout)) {
            match h.iter_mut().find(|(id, _)| *id == s.layout.identity()) {
                Some(entry) => entry.1 += 1,
                None => h.push((s.layout.identity(), 1)),
            }
        }
        h
    };
    let pick = |slides: &[Varied], i: usize, escaping: &str| -> Option<Layout> {
        let position = if slides.len() > 1 { i as f64 / (slides.len() - 1) as f64 } else { 0.0 };
        let (matched, generic) = candidates(&slides[i], position);
        let h = histogram(slides);
        for tier in [matched, generic] {
            for k in 0..tier.len() {
                let t = &tier[(i + k) % tier.len()];
                let id = t.identity();
                if id == escaping || id == slides[i].layout.layout {
                    continue;
                }
                if h.iter().find(|(x, _)| *x == id).map_or(0, |e| e.1) >= cap {
                    continue;
                }
                return Some(t.clone());
            }
        }
        None
    };

    let (mut run, mut length) = ("", 0);
    for i in 0..slides.len() {
        let id = slides[i].layout.identity();
        if id == run {
            length += 1;
        } else {
            run = id;
            length = 1;
        }
        if length < 3 || !swappable(&slides[i]) {
            continue;
        }
        let Some(to) = pick(slides, i, run) else { continue };
        swap(slides, i, to, Reason::RunBreak);
        run = slides[i].layout.identity();
        length = 1;
    }

    for (id, count) in histogram(slides) {
        let mut excess = count.saturating_sub(cap);
        let mut i = slides.len();
        while excess > 0 && i > 0 {
            i -= 1;
            if slides[i].layout.identity() != id || !swappable(&slides[i]) {
                continue;
            }
            let Some(to) = pick(slides, i, id) else { continue };
            swap(slides, i, to, Reason::DominanceCap);
            excess -= 1;
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::LAYOUT_CATALOG;

    /// Vzestup's table: each of the catalog's 56 names, its structural layout and its rich one.
    #[test]
    fn each_catalog_name_maps_as_vzestups_does() {
        let want = [
            ("title", None),
            ("section", None),
            ("agenda", None),
            ("closing", None),
            ("body", None),
            ("numbered", None),
            ("quote", Some("quote")),
            ("statement", None),
            ("definition", None),
            ("metric", Some("metric")),
            ("scorecard", None),
            ("bar-chart", None),
            ("line-chart", None),
            ("two-column", None),
            ("before-after", None),
            ("pros-cons", Some("pros-cons")),
            ("quadrant", None),
            ("steps", None),
            ("timeline", Some("timeline")),
            ("journey", None),
            ("image", None),
            ("image-text", None),
            ("gallery", None),
            ("framework", None),
            ("callout", None),
            ("timeline", Some("roadmap")),
            ("quadrant", Some("matrix")),
            ("body", Some("okrs")),
            ("body", Some("raci")),
            ("quadrant", Some("swot")),
            ("statement", Some("thesis")),
            ("body", Some("letter")),
            ("body", Some("memo")),
            ("body", Some("receipt")),
            ("numbered", Some("next-steps")),
            ("timeline", Some("milestones")),
            ("framework", Some("3horizons")),
            ("framework", Some("funnel")),
            ("framework", Some("scenarios")),
            ("two-column", Some("vs")),
            ("scorecard", Some("kpi-grid")),
            ("numbered", Some("ranking")),
            ("statement", Some("headline")),
            ("statement", Some("manifesto")),
            ("gallery", Some("team")),
            ("image-text", Some("bio")),
            ("quote", Some("testimonials")),
            ("steps", Some("how-it-works")),
            ("steps", Some("pipeline")),
            ("framework", Some("stack")),
            ("two-column", Some("faq")),
            ("two-column", Some("glossary")),
            ("numbered", Some("checklist")),
            ("callout", Some("ask")),
            ("section", Some("qa")),
            ("numbered", Some("recipe-card")),
        ];
        for ((name, _), (layout, spec)) in LAYOUT_CATALOG.iter().zip(want) {
            assert_eq!(of(name), Layout { layout, spec }, "{name}");
        }
        assert_eq!(of("  THANK YOU "), Layout { layout: "closing", spec: Some("thanks") });
        assert_eq!(of("Something else"), Layout { layout: "body", spec: None });
    }

    fn slide(name: &str, title: &str, bullets: &[&str]) -> Slide {
        Slide {
            slide_title: title.into(),
            layout_suggestion: name.into(),
            key_bullets: bullets.iter().map(|b| b.to_string()).collect(),
            ..Slide::default()
        }
    }

    /// Narrato's worked example (§5.5): the closing bookend unwraps its bullet, and the third
    /// body in a row, phase-shaped, becomes a roadmap.
    #[test]
    fn the_worked_example_varies_as_vzestups_does() {
        let mut slides: Vec<Varied> = [
            Slide {
                subtitle: "Series A \u{b7} 2026".into(),
                ..slide("Title", "Carbon-negative cargo is ready to scale", &[])
            },
            slide(
                "Bullet List",
                "Shipping emits three percent of global carbon",
                &["Emissions rising 2% a year", "Regulation tightening in 2027", "Shippers face penalties"],
            ),
            slide(
                "Bullet List",
                "Fines and lost contracts are compounding fast",
                &["Fines of $12M", "Lost contracts worth $28M", "Insurance up 30%"],
            ),
            slide(
                "Bullet List",
                "Phased rollout from pilot to forty lanes",
                &["Phase 1: 5 lanes in Q1", "Phase 2: 15 lanes in Q2", "Phase 3: 40 lanes in Q4"],
            ),
            Slide {
                chart_data: Some(Default::default()),
                ..slide("Bar Chart", "Emissions per lane, 2023\u{2013}2025", &["Down 41%"])
            },
            slide("Statement", "Thank you", &["Join us in the $12M round"]),
        ]
        .iter()
        .map(Varied::of)
        .collect();
        let changes = vary(&mut slides, true);
        assert_eq!(
            changes,
            [
                Change { index: 5, from: "statement", to: "closing", reason: Reason::BookendClosing },
                Change { index: 3, from: "body", to: "roadmap", reason: Reason::RunBreak },
            ]
        );
        assert_eq!(slides[5].body, "Join us in the $12M round");
        assert_eq!(slides[3].layout, Layout { layout: "timeline", spec: Some("roadmap") });
    }
}
