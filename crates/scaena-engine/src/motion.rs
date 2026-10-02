//! A state's motions (SPEC §3.9), resolved against the theme into the timeline's items:
//! its choreography, and what its nodes ask for themselves (an `enter` preset as they
//! enter, an `exit` preset as they leave, and an `emphasis` and `anim` tracks in the
//! state itself). Which nodes enter and leave is read from the snapshots; what a cue
//! splits its targets into is counted after layout, by the transition (SPEC §5).
//!
//! Every setting is taken from the most specific place that gives it: the
//! choreography item, then the preset call (`{ "preset": "grow", "stagger": 40 }`),
//! then the theme's preset, then the theme's `standard` duration and easing. A spring
//! wins over an easing at the same place, and lasts its settle time.

use crate::EngineError;
use crate::theme::Theme;
use scaena_core::document::{NodeType, State};
use scaena_core::model::states::{ChoreoItem, ChoreoTarget, Targets, Timing};
use scaena_core::model::theme::Preset;
use scaena_core::model::values::{
    AnimTracks, Duration, Easing, Keyframe, LookParams, NonNegative, PresetCall, PresetLook, PresetRef, Scale,
    SplitUnit, Spring,
};
use scaena_core::timeline::{Cue, Curve, Item, Key, Keys, Look, Motion};
use scaena_core::{Deck, Snapshot};
use serde_json::Value;

/// What a cue does, before its look is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Enter,
    Exit,
    Emphasis,
    Anim,
}

/// The motions of `state`, entered from `from`: the state before it in the cue list, at
/// rest, or `None` into the first state. Under `match: none` (`matched` false), every
/// node leaves and enters.
pub fn items(
    deck: &Deck,
    theme: &Theme,
    state: &State,
    from: Option<&Snapshot>,
    to: &Snapshot,
    matched: bool,
) -> Result<Vec<Item>, EngineError> {
    let mut items = Vec::new();
    // The nodes the choreography moves, and how: their own presets give way to it.
    let mut named: Vec<(String, Kind)> = Vec::new();
    for (i, value) in state.choreography.iter().enumerate() {
        let item: ChoreoItem = serde_json::from_value(value.clone())
            .map_err(|e| EngineError::Layout(format!("state `{}` choreography {i}: {e}", state.id)))?;
        items.push(choreo(theme, &item, &mut named)?);
    }
    let covered = |id: &str, kind: Kind| named.iter().any(|(n, k)| n == id && *k == kind);
    for (id, node) in &deck.nodes {
        let chart = node.node_type == NodeType::Chart;
        let (now, before) = (to.nodes.get(id), from.and_then(|f| f.nodes.get(id)));
        let stays = matched && now.is_some() && before.is_some();
        // A chart reads its presets for its marks, which enter and leave as its data
        // changes even while the chart stays.
        if let Some(props) = now
            && (!stays || chart)
            && !covered(id, Kind::Enter)
            && let Some(preset) = props.get("enter")
        {
            items.push(Item::Cue(own(theme, id, preset, Kind::Enter, chart, Timing::With)?));
        }
        if let Some(props) = before
            && (!stays || chart)
            && !covered(id, Kind::Exit)
            && let Some(preset) = props.get("exit")
        {
            items.push(Item::Cue(own(theme, id, preset, Kind::Exit, chart, Timing::With)?));
        }
        let Some(props) = now else { continue };
        if !covered(id, Kind::Emphasis)
            && let Some(preset) = props.get("emphasis")
        {
            items.push(Item::Cue(own(theme, id, preset, Kind::Emphasis, false, Timing::After)?));
        }
        if !covered(id, Kind::Anim)
            && let Some(tracks) = props.get("anim")
        {
            let tracks: AnimTracks = serde_json::from_value(tracks.clone())
                .map_err(|e| EngineError::Layout(format!("node `{id}`: `anim` {e}")))?;
            items.push(Item::Cue(keys_cue(theme, vec![id.clone()], &tracks, 0.0, Timing::With)?));
        }
    }
    Ok(items)
}

/// Whether a cue of `items` splits a node into units only layout counts: lines, words,
/// or glyphs of text, or a chart's marks. The global timeline lays such a state out.
pub fn counted_by_layout(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Cue(cue) => cue.split.is_some_and(|s| s != SplitUnit::Children),
        Item::Sequence { items, .. } | Item::Parallel { items, .. } => counted_by_layout(items),
    })
}

/// A node's own `enter`, `exit`, or `emphasis` preset. A chart's apply to its marks, one
/// at a time.
fn own(theme: &Theme, id: &str, preset: &Value, kind: Kind, chart: bool, timing: Timing) -> Result<Cue, EngineError> {
    let preset: PresetRef =
        serde_json::from_value(preset.clone()).map_err(|e| EngineError::Layout(format!("node `{id}`: {e}")))?;
    let mut cue = preset_cue(theme, vec![id.to_string()], &preset, kind, None, timing)?;
    if chart {
        cue.split = Some(SplitUnit::Marks);
    }
    Ok(cue)
}

fn choreo(theme: &Theme, item: &ChoreoItem, named: &mut Vec<(String, Kind)>) -> Result<Item, EngineError> {
    let delay = |d: &Option<NonNegative>| d.as_ref().map_or(0.0, |d| d.0);
    Ok(match item {
        ChoreoItem::Target(t) => Item::Cue(target(theme, t, named)?),
        ChoreoItem::Sequence(s) => Item::Sequence {
            items: s.sequence.iter().map(|i| choreo(theme, i, named)).collect::<Result<_, _>>()?,
            delay: delay(&s.delay),
            timing: s.timing.unwrap_or(Timing::After),
        },
        ChoreoItem::Parallel(p) => Item::Parallel {
            items: p.parallel.iter().map(|i| choreo(theme, i, named)).collect::<Result<_, _>>()?,
            delay: delay(&p.delay),
            timing: p.timing.unwrap_or(Timing::After),
        },
    })
}

/// One choreography item: one motion for one node or several.
fn target(theme: &Theme, t: &ChoreoTarget, named: &mut Vec<(String, Kind)>) -> Result<Cue, EngineError> {
    let targets: Vec<String> = match &t.target {
        Targets::One(id) => vec![id.0.clone()],
        Targets::Many(ids) => ids.0.iter().map(|id| id.0.clone()).collect(),
    };
    let motions = [
        t.enter.as_ref().map(|p| (Kind::Enter, Some(p))),
        t.exit.as_ref().map(|p| (Kind::Exit, Some(p))),
        t.emphasis.as_ref().map(|p| (Kind::Emphasis, Some(p))),
        t.anim.as_ref().map(|_| (Kind::Anim, None)),
    ];
    let mut given = motions.into_iter().flatten();
    let (Some((kind, preset)), None) = (given.next(), given.next()) else {
        let who = targets.join("`, `");
        return Err(EngineError::Layout(format!(
            "choreography for `{who}`: name one motion, `enter`, `exit`, `emphasis`, or `anim`"
        )));
    };
    named.extend(targets.iter().map(|id| (id.clone(), kind)));
    let timing = t.timing.unwrap_or(Timing::After);
    let mut cue = match (preset, &t.anim) {
        (Some(preset), _) => preset_cue(theme, targets, preset, kind, Some(t), timing)?,
        (None, Some(tracks)) => keys_cue(theme, targets, tracks, t.stagger.as_ref().map_or(0.0, |s| s.0), timing)?,
        (None, None) => unreachable!("a motion was named"),
    };
    if let Some(d) = &t.delay {
        cue.delay = d.0;
    }
    if let Some(split) = t.split {
        cue.split = Some(split);
    }
    Ok(cue)
}

/// A cue from a preset, by name or called with settings, and the choreography item that
/// names it, if one does.
fn preset_cue(
    theme: &Theme,
    targets: Vec<String>,
    preset: &PresetRef,
    kind: Kind,
    item: Option<&ChoreoTarget>,
    timing: Timing,
) -> Result<Cue, EngineError> {
    let (name, call) = match preset {
        PresetRef::Named(name) => (name.as_str(), None),
        PresetRef::With(call) => (call.preset.as_str(), Some(&**call)),
    };
    let p = theme.preset(name).ok_or_else(|| EngineError::Theme(format!("no motion preset `{name}`")))?;
    let given = call.and_then(|c| c.params.as_ref());
    let look = |values: Option<&PresetLook>| look(theme, values, given);
    let motion = match kind {
        Kind::Enter => Motion::Enter(look(p.from.as_ref().or(p.to.as_ref()))?),
        Kind::Exit => Motion::Exit(look(p.from.as_ref().or(p.to.as_ref()))?),
        Kind::Emphasis => Motion::Emphasis(look(p.to.as_ref().or(p.from.as_ref()))?),
        Kind::Anim => unreachable!("anim names no preset"),
    };
    let curve = curve(theme, item, call, p)?;
    let eased = match (item.and_then(|t| t.duration.as_ref()), call.and_then(|c| c.duration.as_ref()), &p.duration) {
        (Some(d), _, _) | (None, Some(d), _) | (None, None, Some(d)) => duration(theme, d)?,
        (None, None, None) => standard(theme)?,
    };
    let stagger = (item.and_then(|t| t.stagger.as_ref()))
        .or(call.and_then(|c| c.stagger.as_ref()))
        .or(p.stagger.as_ref())
        .map_or(0.0, |s| s.0);
    Ok(Cue {
        targets,
        split: call.and_then(|c| c.split).or(p.split),
        motion,
        timing,
        delay: call.and_then(|c| c.delay.as_ref()).map_or(0.0, |d| d.0),
        stagger,
        duration: curve.duration(eased),
        curve,
    })
}

/// The curve from the most specific place that names an easing or a spring; at one
/// place, a spring wins.
fn curve(
    theme: &Theme,
    item: Option<&ChoreoTarget>,
    call: Option<&PresetCall>,
    preset: &Preset,
) -> Result<Curve, EngineError> {
    let preset_spring = preset.spring.clone().map(Spring::Named);
    let places = [
        (item.and_then(|t| t.ease.as_ref()), item.and_then(|t| t.spring.as_ref())),
        (call.and_then(|c| c.ease.as_ref()), call.and_then(|c| c.spring.as_ref())),
        (preset.ease.as_ref(), preset_spring.as_ref()),
    ];
    for (ease, spring) in places {
        if let Some(spring) = spring {
            return Ok(Curve::spring(spring_of(theme, spring)?));
        }
        if let Some(ease) = ease {
            return Ok(Curve::Ease(easing(theme, ease)?));
        }
    }
    Ok(Curve::Ease(standard_ease(theme)?))
}

/// `anim` tracks as a cue: keys in each property's own time, from the cue's start.
fn keys_cue(
    theme: &Theme,
    targets: Vec<String>,
    tracks: &AnimTracks,
    stagger: f64,
    timing: Timing,
) -> Result<Cue, EngineError> {
    let mut keys = Keys::default();
    for (property, frames) in &tracks.0 {
        let mut frames: Vec<&Keyframe> = frames.iter().collect();
        frames.sort_by(|a, b| a.t.total_cmp(&b.t));
        let curve = |k: &Keyframe| -> Result<Curve, EngineError> {
            match (&k.spring, &k.ease) {
                (Some(s), _) => Ok(Curve::spring(spring_of(theme, s)?)),
                (None, Some(e)) => Ok(Curve::Ease(easing(theme, e)?)),
                (None, None) => Ok(Curve::Ease(standard_ease(theme)?)),
            }
        };
        let bad = |k: &Keyframe| EngineError::Layout(format!("`anim.{property}` key at {} ms: {}", k.t, k.v));
        match property.as_str() {
            "opacity" | "rotate" => {
                let list = frames
                    .iter()
                    .map(|k| Ok(Key { t: k.t, v: k.v.as_f64().ok_or_else(|| bad(k))?, curve: curve(k)? }))
                    .collect::<Result<Vec<_>, EngineError>>()?;
                if property == "opacity" { keys.opacity = list } else { keys.rotate = list }
            }
            "translate" | "scale" => {
                let list = frames
                    .iter()
                    .map(|k| {
                        Ok(Key { t: k.t, v: pair(&k.v, property == "scale").ok_or_else(|| bad(k))?, curve: curve(k)? })
                    })
                    .collect::<Result<Vec<_>, EngineError>>()?;
                if property == "scale" { keys.scale = list } else { keys.translate = list }
            }
            "progress" => {
                keys.progress = frames
                    .iter()
                    .map(|k| Ok(Key { t: k.t, v: fraction(&k.v).ok_or_else(|| bad(k))?, curve: curve(k)? }))
                    .collect::<Result<Vec<_>, EngineError>>()?;
            }
            _ => {
                return Err(EngineError::Layout(format!(
                    "`anim.{property}`: a track moves `opacity`, `translate`, `scale`, `rotate`, or `progress`"
                )));
            }
        }
    }
    let duration = keys.duration();
    Ok(Cue {
        targets,
        split: None,
        motion: Motion::Keys(keys),
        timing,
        delay: 0.0,
        stagger,
        duration,
        curve: Curve::Ease(scaena_core::timeline::CubicBezier::LINEAR),
    })
}

/// A preset's `from` or `to` as a look: `opacity`; `transform`'s `translate`, `scale`,
/// `rotate` (degrees), and `anchor`; `color`, a theme color its paints mix toward; and
/// `params`, how much of its strokes is drawn. A call's `params` (`given`) win over
/// the preset's.
fn look(theme: &Theme, values: Option<&PresetLook>, given: Option<&LookParams>) -> Result<Look, EngineError> {
    let mut look = Look::REST;
    let values = values.cloned().unwrap_or_default();
    if let Some(opacity) = values.opacity {
        look.opacity = opacity;
    }
    let t = values.transform.unwrap_or_default();
    look.translate = t.translate.unwrap_or(look.translate);
    look.rotate = t.rotate.unwrap_or(look.rotate);
    look.anchor = t.anchor.unwrap_or(look.anchor);
    look.scale = match t.scale {
        Some(Scale::Uniform(k)) => [k, k],
        Some(Scale::Xy(xy)) => xy,
        None => look.scale,
    };
    if let Some(color) = &values.color {
        look.tint = Some((theme.color(&color.0)?, 1.0));
    }
    for params in [values.params.as_ref(), given].into_iter().flatten() {
        look.progress = params.progress.unwrap_or(look.progress);
    }
    Ok(look)
}

/// A number from 0 to 1.
fn fraction(v: &Value) -> Option<f64> {
    v.as_f64().filter(|f| (0.0..=1.0).contains(f))
}

/// `[x, y]`, or with `one`, a single number for both.
fn pair(v: &Value, one: bool) -> Option<[f64; 2]> {
    match v {
        Value::Number(n) if one => n.as_f64().map(|n| [n, n]),
        Value::Array(a) if a.len() == 2 => Some([a[0].as_f64()?, a[1].as_f64()?]),
        _ => None,
    }
}

fn duration(theme: &Theme, d: &Duration) -> Result<f64, EngineError> {
    let v = match d {
        Duration::Ms(ms) => Value::from(*ms),
        Duration::Named(name) => Value::from(name.as_str()),
    };
    theme
        .duration(&v)
        .filter(|d| d.is_finite() && *d >= 0.0)
        .ok_or_else(|| EngineError::Theme(format!("unknown or invalid duration {v}")))
}

fn standard(theme: &Theme) -> Result<f64, EngineError> {
    duration(theme, &Duration::Named("standard".into()))
}

fn easing(theme: &Theme, e: &Easing) -> Result<scaena_core::timeline::CubicBezier, EngineError> {
    match e {
        Easing::Named(name) => theme.easing(name).ok_or_else(|| EngineError::Theme(format!("no easing `{name}`"))),
        Easing::Bezier([a, b, c, d]) => Ok(scaena_core::timeline::CubicBezier(*a, *b, *c, *d)),
    }
}

fn standard_ease(theme: &Theme) -> Result<scaena_core::timeline::CubicBezier, EngineError> {
    easing(theme, &Easing::Named("standard".into()))
}

fn spring_of(theme: &Theme, s: &Spring) -> Result<scaena_core::timeline::Spring, EngineError> {
    match s {
        Spring::Named(name) => theme.spring(name).ok_or_else(|| EngineError::Theme(format!("no spring `{name}`"))),
        Spring::Params(p) => Ok(scaena_core::timeline::Spring {
            stiffness: p.stiffness,
            damping: p.damping,
            mass: p.mass.unwrap_or(1.0),
        }),
    }
}
