//! Pictures for a deck's slides (PLAN G.10, ADR-0026): Vzestup's supporting images, each made by
//! the user's own image model from the slide's visual concept, with no word in it, so a slide's
//! words stay the engine's to set. Which slides get one and the prompt each is made from are
//! Vzestup's (`editablePptx/anthropic.ts`); each request is OpenAI's Images API or Gemini's,
//! the key in its headers and nowhere else. No HTTP: the client makes each call, and reads
//! each answer back here.

use crate::providers::{Provider, Request, at, with_json};
use crate::{Slide, js};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The most pictures one deck asks for (Vzestup's `MAX_SUPPORTING_IMAGES`).
pub const MOST: usize = 10;

/// What a picture is made as: an editorial illustration in the theme's palette (Vzestup's), or
/// an editorial photograph graded to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    #[default]
    Illustration,
    Photo,
}

impl Look {
    /// The look `name` names (`illustration`, `photo`).
    pub fn named(name: &str) -> Option<Look> {
        match name.trim().to_ascii_lowercase().as_str() {
            "illustration" | "illustrations" => Some(Look::Illustration),
            "photo" | "photos" | "photograph" | "photographs" => Some(Look::Photo),
            _ => None,
        }
    }

    fn medium(self) -> &'static str {
        match self {
            Look::Illustration => "illustration",
            Look::Photo => "photograph",
        }
    }
}

/// How much care, and cost, each picture is made with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Low,
    #[default]
    Medium,
    High,
}

impl Quality {
    pub fn named(name: &str) -> Option<Quality> {
        match name.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Quality::Low),
            "medium" => Some(Quality::Medium),
            "high" => Some(Quality::High),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::Medium => "medium",
            Quality::High => "high",
        }
    }
}

/// The shape of a picture's place on its slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frame {
    Landscape,
    Portrait,
    Square,
}

impl Frame {
    /// The frame a place `width` × `height` takes.
    pub fn of(width: f64, height: f64) -> Frame {
        let ratio = if height > 0.0 { width / height } else { 1.0 };
        if ratio >= 1.2 {
            Frame::Landscape
        } else if ratio <= 0.84 {
            Frame::Portrait
        } else {
            Frame::Square
        }
    }

    fn word(self) -> &'static str {
        match self {
            Frame::Landscape => "Landscape",
            Frame::Portrait => "Portrait",
            Frame::Square => "Square",
        }
    }

    /// The size OpenAI's GPT Image models make it at.
    fn gpt_size(self) -> &'static str {
        match self {
            Frame::Landscape => "1536x1024",
            Frame::Portrait => "1024x1536",
            Frame::Square => "1024x1024",
        }
    }

    /// The size DALL·E 3 makes it at.
    fn dalle_size(self) -> &'static str {
        match self {
            Frame::Landscape => "1792x1024",
            Frame::Portrait => "1024x1792",
            Frame::Square => "1024x1024",
        }
    }

    /// Gemini's and Imagen's aspect ratio for it.
    fn aspect(self) -> &'static str {
        match self {
            Frame::Landscape => "16:9",
            Frame::Portrait => "3:4",
            Frame::Square => "1:1",
        }
    }
}

/// The theme a picture is made to sit in: its name, and its ground, ink, and accent colors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub ground: String,
    pub ink: String,
    pub accent: String,
}

impl Palette {
    /// Whether its ground is dark: the pictures then read on a dark ground.
    pub fn dark(&self) -> bool {
        let hex = self.ground.trim_start_matches('#');
        let channel =
            |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255) as f64 / 255.0;
        let luminance = 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
        luminance < 0.4
    }
}

/// How much a slide in a Narrato layout of `identity` wants a picture (Vzestup's priorities,
/// kept to the layouts Scaena draws a picture beside words in): a cover, a divider, an ending,
/// and a picture slide most; a list next; a big number and an agenda then; none for a chart, a
/// table, a quote, or a slide of places, where a picture is decoration.
pub fn priority(identity: &str) -> u8 {
    match identity {
        "image" | "title" | "section" | "closing" | "thanks" | "image-text" => 3,
        "body" => 2,
        "metric" | "agenda" => 1,
        _ => 0,
    }
}

/// The slides that get a picture, in the deck's order, each a Narrato layout's `identity`: those
/// that want one most first, up to two in five of the deck's slides, ten at most (Vzestup's cap).
pub fn wanted(identities: &[&str]) -> Vec<usize> {
    let cap = MOST.min((identities.len() * 2).div_ceil(5).max(1));
    let mut ranked: Vec<(u8, usize)> =
        identities.iter().enumerate().map(|(i, id)| (priority(id), i)).filter(|(p, _)| *p > 0).collect();
    // Most wanted first, then earlier.
    ranked.sort_by_key(|(p, i)| (std::cmp::Reverse(*p), *i));
    let mut picked: Vec<usize> = ranked.into_iter().take(cap).map(|(_, i)| i).collect();
    picked.sort_unstable();
    picked
}

/// What a slide's picture shows (Vzestup's): its visual concept, else its title, its words, and
/// its fact, else its title, else an abstract concept.
pub fn subject(slide: &Slide) -> String {
    let concept = slide.visual_description.trim();
    if !concept.is_empty() {
        return concept.to_string();
    }
    let body = slide.key_bullets.join("; ");
    let parts: Vec<&str> =
        [slide.slide_title.trim(), body.trim(), slide.supporting_facts.as_deref().unwrap_or("").trim()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect();
    if !parts.is_empty() {
        return js::first_units(&parts.join(" \u{2014} "), 600);
    }
    "Abstract concept illustration".into()
}

/// How a picture for a slide in a Narrato layout of `identity` is composed (Vzestup's
/// `compositionForLayout`).
pub fn composition(identity: &str) -> &'static str {
    match identity {
        "title" | "section" | "closing" | "thanks" => {
            "a dramatic, wide, atmospheric conceptual composition with depth \u{2014} the hero image of the deck, cinematic and bold."
        }
        "metric" | "scorecard" => {
            "a single bold, iconic, graphic motif that symbolizes the figure \u{2014} clean, high-impact, poster-like, minimal detail."
        }
        "quote" | "statement" => {
            "an evocative human figure or atmospheric scene that carries emotional weight \u{2014} intimate and expressive."
        }
        "timeline" | "journey" | "steps" => {
            "a composition with a clear sense of progression or directional movement through the subject."
        }
        "framework" | "quadrant" => "a clean, structured, diagrammatic subject that reads as an ordered system.",
        _ => "one clear central subject, cleanly and confidently composed.",
    }
}

/// The style a deck's pictures share, in its theme's palette: Vzestup's art direction drawn from
/// a theme's tokens for an illustration, and its photographic counterpart.
pub fn style(look: Look, palette: &Palette) -> String {
    let (name, ground, ink, accent) = (&palette.name, &palette.ground, &palette.ink, &palette.accent);
    match look {
        Look::Illustration => {
            let mood = if palette.dark() {
                "moody, low-key and atmospheric, reading on a dark ground"
            } else {
                "bright, airy and light-filled, reading on a light ground"
            };
            format!(
                "A single bold, confident editorial illustration to match a \"{name}\" presentation.\n\
                 STYLE \u{2014} a confident, considered editorial-illustration feel.\n\
                 PALETTE \u{2014} {ink} for line/subject, {accent} as the one bold accent, on {ground}. No other colors.\n\
                 MOOD \u{2014} {mood}.\n\
                 AVOID \u{2014} not a pale blueprint, thin technical line drawing, faint watercolor on white, or clipart; confident and on-brand."
            )
        }
        Look::Photo => {
            let mood = if palette.dark() {
                "moody, low-key and cinematic, reading on a dark ground"
            } else {
                "bright, airy and natural, reading on a light ground"
            };
            format!(
                "Editorial photography to match a \"{name}\" presentation: one real-looking scene, natural light, true textures, a considered depth of field.\n\
                 PALETTE \u{2014} graded toward {ground} in the shadows and {accent} in the highlights, so it sits in the deck's colors.\n\
                 MOOD \u{2014} {mood}.\n\
                 AVOID \u{2014} stock-photo clich\u{e9}s, posed smiles, staged handshakes, heavy filters, HDR, and any visible sign or screen."
            )
        }
    }
}

/// The prompt for a picture of `subject` (Vzestup's supporting image): in `style`, composed as
/// `composition` says, its frame `frame`, with no word in it and no one anyone would know.
pub fn prompt(look: Look, style: &str, subject: &str, composition: &str, frame: Frame) -> String {
    let medium = look.medium();
    format!(
        "A single-subject image whose medium and rendering are defined by STYLE\n\
         below (follow it exactly \u{2014} do not default to a sketch). NO text in the image.\n\
         \n\
         STYLE:\n\
         {style}\n\
         \n\
         SUBJECT: {subject}\n\
         \n\
         COMPOSITION: {composition}\n\
         \n\
         RULES (strict):\n\
         - ZERO text, words, letters, numbers, labels, or annotations in the\n  image. No callout text, no caption, no watermark.\n\
         - No slide framing, no presentation borders, no title bar, no header,\n  no footer.\n\
         - ONE clear, legible focal subject \u{2014} a single unified {medium}.\n  NO montage, collage, multi-panel composite, grid of vignettes, or\n  split scenes; do NOT cram several small scenes together. One subject,\n  rendered well, that supports the slide's topic; it will sit BESIDE\n  body text on a slide, so it must not try to be the whole slide.\n\
         - {} composition, content centered, generous margin so it crops\n  cleanly to any rectangular region.\n\
         - Subject only \u{2014} pure {medium}, no UI chrome, no device mockups\n  unless the subject IS a device.\n\
         - No recognizable real person, public figure, brand, logo, or trademark.",
        frame.word()
    )
}

/// Who makes a deck's pictures: an image model the user's key lists, at a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub provider: Provider,
    pub model: String,
    /// The API's address, where it is not the provider's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default)]
    pub quality: Quality,
}

/// Whether `model` is a model of `provider`'s that makes pictures.
pub fn paints(provider: Provider, model: &str) -> bool {
    match provider {
        Provider::Openai => model.starts_with("gpt-image") || model.starts_with("dall-e-3"),
        Provider::Gemini => model.starts_with("imagen") || (model.starts_with("gemini") && model.contains("image")),
        Provider::Anthropic => false,
    }
}

/// Whether `id` reads as a model's name: letters, digits, `.`, `_`, and `-`.
pub fn named(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// The image models an answer to [`Provider::models`] lists, best first: OpenAI's GPT Image
/// models newest first, its minis after them, then DALL·E 3; Gemini's Imagen models, then its
/// Gemini image models. Anthropic makes none.
pub fn models(provider: Provider, status: u16, status_text: &str, body: &str) -> Result<Vec<String>, String> {
    let body = crate::providers::read(status, status_text, body)?;
    let listed = |key: &str| body.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut ids: Vec<String> = match provider {
        Provider::Openai => {
            listed("data").iter().filter_map(|m| m.get("id").and_then(Value::as_str)).map(str::to_string).collect()
        }
        Provider::Gemini => (listed("models").iter())
            .filter_map(|m| m.get("name").and_then(Value::as_str))
            .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
            .collect(),
        Provider::Anthropic => Vec::new(),
    };
    // A name is the URL's path where it is asked, so one that would change the path is no model.
    ids.retain(|id| named(id) && paints(provider, id));
    // A version from the name: gpt-image-2.5-… is 2.5; dall-e-3 and Gemini's sort after.
    let version = |id: &str| -> f64 {
        let digits: String = id
            .strip_prefix("gpt-image-")
            .or_else(|| id.strip_prefix("imagen-"))
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        digits.trim_end_matches('.').parse().unwrap_or(0.0)
    };
    let rank = |id: &str| -> (u8, bool, i64, usize) {
        let family = match provider {
            Provider::Openai if id.starts_with("gpt-image") => 0,
            Provider::Gemini if id.starts_with("imagen") => 0,
            _ => 1,
        };
        let lesser = id.contains("mini") || id.contains("fast") || id.contains("lite");
        (family, lesser, -(version(id) * 100.0) as i64, id.len())
    };
    ids.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    ids.dedup();
    Ok(ids)
}

/// The request that makes a picture from `prompt`, `frame` shaped, with `key`; `plain` asks
/// OpenAI for a PNG where it would ask for a JPEG, for a model that makes no JPEG.
pub fn request(settings: &Settings, key: &str, prompt: &str, frame: Frame, plain: bool) -> Request {
    let model = settings.model.as_str();
    let base = settings.base.as_deref();
    match settings.provider {
        Provider::Openai | Provider::Anthropic => {
            let body = if model.starts_with("dall-e") {
                let quality = if settings.quality == Quality::High { "hd" } else { "standard" };
                json!({ "model": model, "prompt": prompt, "n": 1, "size": frame.dalle_size(), "quality": quality, "response_format": "b64_json" })
            } else if plain {
                json!({ "model": model, "prompt": prompt, "n": 1, "size": frame.gpt_size(), "quality": settings.quality.word() })
            } else {
                // A JPEG, a tenth of a PNG's bytes for a picture with no clear part.
                json!({
                    "model": model, "prompt": prompt, "n": 1, "size": frame.gpt_size(),
                    "quality": settings.quality.word(), "output_format": "jpeg", "output_compression": 90,
                })
            };
            let headers = with_json(BTreeMap::from([("authorization".into(), format!("Bearer {key}"))]));
            Request {
                method: "POST".into(),
                url: at(base, Provider::Openai.base(), "/v1/images/generations"),
                headers,
                body: Some(body),
            }
        }
        Provider::Gemini => {
            let headers = with_json(BTreeMap::from([("x-goog-api-key".into(), key.to_string())]));
            let (path, body) = if model.starts_with("imagen") {
                (
                    format!("/v1beta/models/{}:predict", crate::providers::component(model)),
                    json!({ "instances": [{ "prompt": prompt }], "parameters": { "sampleCount": 1, "aspectRatio": frame.aspect() } }),
                )
            } else {
                (
                    format!("/v1beta/models/{}:generateContent", crate::providers::component(model)),
                    json!({
                        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
                        "generationConfig": { "responseModalities": ["IMAGE"], "imageConfig": { "aspectRatio": frame.aspect() } },
                    }),
                )
            };
            Request { method: "POST".into(), url: at(base, Provider::Gemini.base(), &path), headers, body: Some(body) }
        }
    }
}

/// A picture made: its bytes, and their kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub bytes: Vec<u8>,
    /// `jpg` or `png`.
    pub ext: &'static str,
}

/// Why a picture was not made.
#[derive(Debug, Clone, PartialEq)]
pub enum Unmade {
    /// The provider is busy or failed for now: ask again, after `after` seconds if it says.
    Again { why: String, after: Option<f64> },
    /// The model makes no JPEG: ask again for a PNG ([`request`]'s `plain`).
    Plain(String),
    /// This picture is not made: refused as unsafe, or nothing came back. The rest go on.
    Skipped(String),
    /// No picture can be made with this key now: out of credit, or not let in. Ask no more.
    Stopped(String),
}

impl std::fmt::Display for Unmade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unmade::Again { why, .. } | Unmade::Plain(why) | Unmade::Skipped(why) | Unmade::Stopped(why) => {
                f.write_str(why)
            }
        }
    }
}

/// The statuses a provider answers with for now, not for good (Vzestup's retry list: a rate
/// limit, a server's failure, and Cloudflare's edge before OpenAI's).
const AGAIN: [u16; 8] = [429, 500, 502, 503, 504, 520, 522, 524];

/// The wait a rate limit's words ask for: "Please try again in 12s", "in 850ms".
fn try_again_in(text: &str) -> Option<f64> {
    let rest = &text[text.find("try again in ")? + "try again in ".len()..];
    let number: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let value: f64 = number.parse().ok()?;
    let unit = &rest[number.len()..];
    Some(if unit.starts_with("ms") {
        value / 1000.0
    } else if unit.starts_with('m') && !unit.starts_with("ms") {
        value * 60.0
    } else {
        value
    })
}

/// The picture an answer to [`request`] holds, or why there is none. `after` is the answer's
/// `Retry-After`, where it has one.
pub fn read(
    provider: Provider,
    status: u16,
    status_text: &str,
    body: &str,
    after: Option<&str>,
) -> Result<Picture, Unmade> {
    let name = if provider == Provider::Gemini { "Gemini" } else { "OpenAI" };
    if !(200..300).contains(&status) {
        let said = crate::providers::read(status, status_text, body)
            .err()
            .unwrap_or_else(|| format!("{status} {status_text}"));
        let lower = body.to_lowercase();
        // Out of credit is a 429 too, which waiting does not mend (Vzestup's signatures).
        if [
            "insufficient_quota",
            "billing_hard_limit_reached",
            "exceeded your current quota",
            "check your plan and billing",
        ]
        .iter()
        .any(|s| lower.contains(s))
        {
            return Err(Unmade::Stopped(format!(
                "{name} is out of credit for pictures: top up its balance. It said: {said}"
            )));
        }
        if AGAIN.contains(&status) {
            let after = after.and_then(|a| a.trim().parse().ok()).or_else(|| try_again_in(&lower));
            return Err(Unmade::Again { why: said, after });
        }
        if matches!(status, 401 | 403) {
            return Err(Unmade::Stopped(format!("{name} would not make pictures with this key: {said}")));
        }
        if status == 400 && (lower.contains("output_format") || lower.contains("output_compression")) {
            return Err(Unmade::Plain(said));
        }
        if lower.contains("moderation") || lower.contains("safety") {
            return Err(Unmade::Skipped(format!("{name}'s safety system refused it: {said}")));
        }
        return Err(Unmade::Skipped(said));
    }
    let Ok(answer) = serde_json::from_str::<Value>(body) else {
        return Err(Unmade::Skipped(format!("{name}'s answer was not JSON: {}", js::first_units(body, 200))));
    };
    let found: Option<(&str, &str)> = match provider {
        Provider::Gemini => answer
            .pointer("/predictions/0/bytesBase64Encoded")
            .and_then(Value::as_str)
            .map(|d| (d, answer.pointer("/predictions/0/mimeType").and_then(Value::as_str).unwrap_or("image/png")))
            .or_else(|| {
                let parts = answer.pointer("/candidates/0/content/parts").and_then(Value::as_array)?;
                parts.iter().find_map(|p| {
                    let inline = p.get("inlineData").or_else(|| p.get("inline_data"))?;
                    let data = inline.get("data").and_then(Value::as_str)?;
                    let kind = inline.get("mimeType").or_else(|| inline.get("mime_type")).and_then(Value::as_str);
                    Some((data, kind.unwrap_or("image/png")))
                })
            }),
        _ => answer.pointer("/data/0/b64_json").and_then(Value::as_str).map(|d| (d, "")),
    };
    let Some((data, kind)) = found else {
        let reason =
            answer.pointer("/candidates/0/finishReason").and_then(Value::as_str).unwrap_or("no picture came back");
        return Err(Unmade::Skipped(format!("{name} made no picture: {reason}")));
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|e| Unmade::Skipped(format!("{name}'s picture did not decode: {e}")))?;
    // What the bytes are, read from them: a JPEG or a PNG, whatever the answer called it.
    let ext = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "jpg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "png"
    } else {
        return Err(Unmade::Skipped(format!("{name} made a picture of a kind a slide does not show ({kind})")));
    };
    Ok(Picture { bytes, ext })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_in_five_slides_get_a_picture_those_that_want_one_most_first() {
        let deck = ["title", "agenda", "section", "metric", "line-chart", "body", "section", "quote", "closing"];
        // Nine slides: four pictures, the cover, the dividers, and the ending.
        assert_eq!(wanted(&deck), vec![0, 2, 6, 8]);
        let long: Vec<&str> = (0..40).map(|i| if i % 2 == 0 { "section" } else { "body" }).collect();
        assert_eq!(wanted(&long).len(), MOST);
        assert_eq!(wanted(&["bar-chart", "quote"]), Vec::<usize>::new());
        assert_eq!(wanted(&["metric"]), vec![0]);
    }

    #[test]
    fn a_prompt_is_vzestups_with_no_word_and_no_one_known_in_it() {
        let dusk =
            Palette { name: "Dusk".into(), ground: "#111217".into(), ink: "#F2F0E9".into(), accent: "#FF6A3D".into() };
        assert!(dusk.dark());
        let style = style(Look::Illustration, &dusk);
        assert!(
            style.contains("PALETTE \u{2014} #F2F0E9 for line/subject, #FF6A3D as the one bold accent, on #111217.")
        );
        assert!(style.contains("moody, low-key and atmospheric"));
        let p =
            prompt(Look::Illustration, &style, "A melting glacier at dawn", composition("section"), Frame::Portrait);
        assert!(p.starts_with("A single-subject image whose medium and rendering are defined by STYLE"));
        assert!(p.contains("SUBJECT: A melting glacier at dawn"));
        assert!(p.contains("COMPOSITION: a dramatic, wide, atmospheric conceptual composition"));
        assert!(p.contains("- ZERO text, words, letters, numbers"));
        assert!(p.contains("- Portrait composition, content centered"));
        assert!(p.contains("No recognizable real person, public figure, brand, logo, or trademark."));
        let daybreak = Palette {
            name: "Daybreak".into(),
            ground: "#FAF7F0".into(),
            ink: "#1A1A1A".into(),
            accent: "#2F6FEB".into(),
        };
        assert!(!daybreak.dark());
        assert!(style_photo_reads_light(&daybreak));
        let mut s = Slide { slide_title: "Pledges".into(), ..Slide::default() };
        assert_eq!(subject(&s), "Pledges");
        s.visual_description = "  Sunrise over a valley ".into();
        assert_eq!(subject(&s), "Sunrise over a valley");
    }

    fn style_photo_reads_light(p: &Palette) -> bool {
        let s = style(Look::Photo, p);
        s.contains("bright, airy and natural") && s.contains("graded toward #FAF7F0 in the shadows and #2F6FEB")
    }

    #[test]
    fn each_provider_is_asked_as_its_api_asks() {
        let openai =
            Settings { provider: Provider::Openai, model: "gpt-image-1".into(), base: None, quality: Quality::Medium };
        let r = request(&openai, "sk-test", "a glacier", Frame::Landscape, false);
        assert_eq!(r.url, "https://api.openai.com/v1/images/generations");
        assert_eq!(r.headers["authorization"], "Bearer sk-test");
        let body = r.body.unwrap();
        assert_eq!((body["size"].as_str(), body["output_format"].as_str()), (Some("1536x1024"), Some("jpeg")));
        let plain = request(&openai, "sk-test", "a glacier", Frame::Portrait, true).body.unwrap();
        assert!(plain.get("output_format").is_none() && plain["size"] == "1024x1536");
        let dalle = Settings { model: "dall-e-3".into(), ..openai.clone() };
        let body = request(&dalle, "sk-test", "a glacier", Frame::Portrait, false).body.unwrap();
        assert_eq!((body["size"].as_str(), body["response_format"].as_str()), (Some("1024x1792"), Some("b64_json")));
        let imagen = Settings {
            provider: Provider::Gemini,
            model: "imagen-4.0-generate-001".into(),
            base: None,
            quality: Quality::Medium,
        };
        let r = request(&imagen, "g-key", "a glacier", Frame::Portrait, false);
        assert!(r.url.ends_with("/v1beta/models/imagen-4.0-generate-001:predict"));
        assert_eq!(r.headers["x-goog-api-key"], "g-key");
        assert_eq!(r.body.unwrap()["parameters"]["aspectRatio"], "3:4");
        let flash = Settings { model: "gemini-2.5-flash-image".into(), ..imagen };
        let body = request(&flash, "g-key", "a glacier", Frame::Square, false).body.unwrap();
        assert_eq!(body["generationConfig"]["responseModalities"][0], "IMAGE");
        assert_eq!(body["generationConfig"]["imageConfig"]["aspectRatio"], "1:1");
    }

    #[test]
    fn a_models_name_never_changes_the_path_it_is_asked_at() {
        let odd = |model: &str| Settings {
            provider: Provider::Gemini,
            model: model.into(),
            base: None,
            quality: Quality::Medium,
        };
        let r = request(&odd("imagen-4/../../files?x=1#"), "g-key", "a glacier", Frame::Square, false);
        assert!(r.url.ends_with("/v1beta/models/imagen-4%2F..%2F..%2Ffiles%3Fx%3D1%23:predict"), "{}", r.url);
        let r = request(&odd("gemini-image/../x"), "g-key", "a glacier", Frame::Square, false);
        assert!(r.url.ends_with("/v1beta/models/gemini-image%2F..%2Fx:generateContent"), "{}", r.url);
        // A listing's name that would change a path is no model.
        let listed =
            json!({ "models": [{ "name": "models/imagen-4/../x" }, { "name": "models/imagen-4.0-generate-001" }] });
        assert_eq!(models(Provider::Gemini, 200, "OK", &listed.to_string()).unwrap(), ["imagen-4.0-generate-001"]);
        assert!(named("gpt-image-1.5") && !named("imagen?x") && !named(""));
    }

    #[test]
    fn an_answer_is_read_to_its_picture_or_why_there_is_none() {
        let jpeg = [0xffu8, 0xd8, 0xff, 0xe0, 0, 1];
        let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
        let made =
            read(Provider::Openai, 200, "OK", &json!({ "data": [{ "b64_json": b64 }] }).to_string(), None).unwrap();
        assert_eq!((made.ext, made.bytes.as_slice()), ("jpg", &jpeg[..]));
        let png = [0x89u8, b'P', b'N', b'G', 13, 10];
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        let gemini = json!({ "candidates": [{ "content": { "parts": [{ "text": "here" }, { "inlineData": { "mimeType": "image/png", "data": b64 } }] } }] });
        assert_eq!(read(Provider::Gemini, 200, "OK", &gemini.to_string(), None).unwrap().ext, "png");
        let imagen = json!({ "predictions": [{ "bytesBase64Encoded": b64, "mimeType": "image/png" }] });
        assert_eq!(read(Provider::Gemini, 200, "OK", &imagen.to_string(), None).unwrap().ext, "png");

        let quota = r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota"}}"#;
        assert!(matches!(read(Provider::Openai, 429, "Too Many Requests", quota, None), Err(Unmade::Stopped(_))));
        let soon = r#"{"error":{"message":"Rate limit reached for images per minute. Please try again in 12s."}}"#;
        assert!(matches!(
            read(Provider::Openai, 429, "Too Many Requests", soon, None),
            Err(Unmade::Again { after: Some(12.0), .. })
        ));
        let busy = r#"{"error":{"message":"Rate limit reached"}}"#;
        assert_eq!(
            read(Provider::Openai, 429, "Too Many Requests", busy, Some("7")),
            Err(Unmade::Again { why: "429 Too Many Requests: Rate limit reached".into(), after: Some(7.0) })
        );
        let format = r#"{"error":{"message":"Unknown parameter: 'output_format'."}}"#;
        assert!(matches!(read(Provider::Openai, 400, "Bad Request", format, None), Err(Unmade::Plain(_))));
        let unsafe_ =
            r#"{"error":{"message":"Your request was rejected by the safety system.","code":"moderation_blocked"}}"#;
        assert!(matches!(read(Provider::Openai, 400, "Bad Request", unsafe_, None), Err(Unmade::Skipped(_))));
        assert!(matches!(read(Provider::Openai, 401, "Unauthorized", "{}", None), Err(Unmade::Stopped(_))));
        let blocked = json!({ "candidates": [{ "finishReason": "IMAGE_SAFETY", "content": { "parts": [] } }] });
        assert_eq!(
            read(Provider::Gemini, 200, "OK", &blocked.to_string(), None),
            Err(Unmade::Skipped("Gemini made no picture: IMAGE_SAFETY".into()))
        );
    }

    #[test]
    fn a_keys_image_models_are_listed_best_first() {
        let listed = json!({ "data": [
            { "id": "gpt-4o" }, { "id": "dall-e-3" }, { "id": "gpt-image-1-mini" }, { "id": "gpt-image-1" },
            { "id": "gpt-image-2" }, { "id": "dall-e-2" }, { "id": "gpt-image-1.5" },
        ] });
        let ids = models(Provider::Openai, 200, "OK", &listed.to_string()).unwrap();
        assert_eq!(ids, ["gpt-image-2", "gpt-image-1.5", "gpt-image-1", "gpt-image-1-mini", "dall-e-3"]);
        let listed = json!({ "models": [
            { "name": "models/gemini-2.5-pro" }, { "name": "models/imagen-4.0-fast-generate-001" },
            { "name": "models/imagen-4.0-generate-001" }, { "name": "models/gemini-2.5-flash-image" },
        ] });
        let ids = models(Provider::Gemini, 200, "OK", &listed.to_string()).unwrap();
        assert_eq!(ids, ["imagen-4.0-generate-001", "imagen-4.0-fast-generate-001", "gemini-2.5-flash-image"]);
        assert!(models(Provider::Anthropic, 200, "OK", r#"{"data":[]}"#).unwrap().is_empty());
        assert_eq!(Frame::of(800.0, 1080.0), Frame::Portrait);
        assert_eq!(Frame::of(1920.0, 1080.0), Frame::Landscape);
        assert_eq!(Frame::of(1000.0, 1080.0), Frame::Square);
    }
}
