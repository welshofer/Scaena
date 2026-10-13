//! Shadows (PLAN 2.115): the shadow a node casts, as a presentation app sets an object's, drawn
//! under it as a rounded rectangle blurred by a Gaussian ([`Op::Shadow`]). A filled rectangle or
//! ellipse, an image, and a container's panel cast one; every painter draws it as it is. An oval's
//! is a circle's stretched to it, so its outline is the oval's own.

use crate::charts::RoundRect;
use crate::error::EngineError;
use crate::theme::Theme;
use scaena_core::displaylist::{Blend, Color, FillRule, Op, Paint, Rect};
use serde_json::Value;

/// A shadow, resolved: how far it falls from what casts it, canvas units; how soft its edge is,
/// the Gaussian's deviation (half the blur's radius a theme writes); and its color at its
/// opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub offset: [f32; 2],
    pub blur: f32,
    pub color: Color,
}

impl Shadow {
    /// The shadow a node's `shadow` names: one of `theme`'s by name (`tokens.shadow`), or one
    /// written out, `x`, `y`, and `blur` lengths, a `color`, and an `opacity`. None where it names
    /// none, or one that cannot be seen.
    pub fn of(theme: &Theme, value: Option<&Value>) -> Result<Option<Shadow>, EngineError> {
        let (x, y, blur, color, opacity) = match value {
            None | Some(Value::Null) => return Ok(None),
            Some(Value::String(name)) => {
                let s = theme.shadow(name)?;
                (s.x as f32, s.y as f32, s.blur.0 as f32, s.color.as_ref().map(|c| c.0.clone()), s.opacity as f32)
            }
            Some(Value::Object(o)) => {
                let length = |key: &str| o.get(key).map_or(Ok(0.0), |v| theme.length(v, 0.0));
                let opacity = match o.get("opacity") {
                    None => 1.0,
                    Some(v) => v
                        .as_f64()
                        .filter(|o| (0.0..=1.0).contains(o))
                        .ok_or_else(|| EngineError::Layout(format!("shadow `opacity` {v}: a number from 0 to 1")))?
                        as f32,
                };
                let color = match o.get("color") {
                    None => None,
                    Some(Value::String(c)) => Some(c.clone()),
                    Some(v) => return Err(EngineError::Layout(format!("shadow `color` {v}: a color"))),
                };
                if let Some(key) = o.keys().find(|k| !["x", "y", "blur", "color", "opacity"].contains(&k.as_str())) {
                    return Err(EngineError::Layout(format!(
                        "shadow `{key}`: a shadow is `x`, `y`, `blur`, `color`, and `opacity`"
                    )));
                }
                (length("x")?, length("y")?, length("blur")?.max(0.0), color, opacity)
            }
            Some(v) => {
                return Err(EngineError::Layout(format!(
                    "`shadow` {v}: the name of one of the theme's shadows, or one written out"
                )));
            }
        };
        let [r, g, b, a] = match color {
            Some(c) => theme.color(&c)?.0,
            None => [0, 0, 0, 255],
        };
        let alpha = (f32::from(a) * opacity.clamp(0.0, 1.0)).round() as u8;
        let finite = [x, y, blur].iter().all(|v| v.is_finite());
        Ok((alpha > 0 && finite).then_some(Shadow { offset: [x, y], blur: blur / 2.0, color: Color([r, g, b, alpha]) }))
    }

    /// What draws it under the rounded rectangle `rect`, its corners of `radius`, in the layer's
    /// space of what casts it: a shadow with no blur is its box, hard-edged, a fill.
    pub fn op(&self, [x, y, w, h]: Rect, radius: f32) -> Op {
        let rect = [x + self.offset[0], y + self.offset[1], w, h];
        let radius = radius.max(0.0).min(w.min(h).max(0.0) / 2.0);
        if self.blur < 0.05 {
            let path = RoundRect::rounded(rect[0], rect[1], w, h, radius).path();
            return Op::Fill { path, rule: FillRule::NonZero, paint: Paint::Solid(self.color) };
        }
        Op::Shadow { rect, radius, blur: self.blur, color: self.color }
    }

    /// What draws it under the ellipse that fills `[x, y, w, h]`: a circle's shadow as wide as
    /// the ellipse is narrow, stretched along its long side to the ellipse. Its outline is the
    /// ellipse's own; its blur is as written across the short side and stretched with it along the
    /// long, and it falls as far as it would from a box.
    pub fn oval(&self, [x, y, w, h]: Rect) -> Op {
        let d = w.min(h);
        if !d.is_finite() || d <= 0.0 || (w - h).abs() <= f32::EPSILON * w.max(h) {
            return self.op([x, y, w, h], d.max(0.0) / 2.0);
        }
        let (sx, sy) = (w / d, h / d);
        let circle = Shadow { offset: [self.offset[0] / sx, self.offset[1] / sy], ..*self };
        Op::Layer {
            node: None,
            cell: None,
            transform: [sx, 0.0, 0.0, sy, x, y],
            opacity: 1.0,
            blend: Blend::Normal,
            clip: None,
            ops: vec![circle.op([0.0, 0.0, d, d], d / 2.0)],
        }
    }

    /// `a` toward `b`, `p` of the way, as a morph moves what casts it: where only one side casts
    /// one, it fades in or out where it falls.
    pub fn between(a: Option<Shadow>, b: Option<Shadow>, p: f32) -> Option<Shadow> {
        let lerp = |x: f32, y: f32| x + (y - x) * p;
        let fade = |s: Shadow, k: f32| Shadow {
            color: Color([s.color.0[0], s.color.0[1], s.color.0[2], (f32::from(s.color.0[3]) * k).round() as u8]),
            ..s
        };
        match (a, b) {
            (Some(a), Some(b)) => {
                let [ca, cb] = [a.color.0, b.color.0];
                let mix = |k: usize| lerp(f32::from(ca[k]), f32::from(cb[k])).round() as u8;
                Some(Shadow {
                    offset: [lerp(a.offset[0], b.offset[0]), lerp(a.offset[1], b.offset[1])],
                    blur: lerp(a.blur, b.blur),
                    color: Color([mix(0), mix(1), mix(2), mix(3)]),
                })
            }
            (Some(a), None) => Some(fade(a, 1.0 - p)),
            (None, Some(b)) => Some(fade(b, p)),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn theme() -> Theme {
        let mut doc: Value =
            serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        doc["tokens"]["shadow"] = json!({ "soft": { "y": 8, "blur": 24, "opacity": 0.25 } });
        Theme::from_json(&doc.to_string()).unwrap()
    }

    #[test]
    fn a_shadow_is_the_themes_by_name_or_one_written_out() {
        let theme = theme();
        let soft = Shadow::of(&theme, Some(&json!("soft"))).unwrap().unwrap();
        assert_eq!(soft, Shadow { offset: [0.0, 8.0], blur: 12.0, color: Color([0, 0, 0, 64]) });
        let written =
            Shadow::of(&theme, Some(&json!({ "x": 4, "y": 4, "blur": 10, "color": "accent", "opacity": 0.5 })));
        let written = written.unwrap().unwrap();
        assert_eq!((written.offset, written.blur, written.color.0[3]), ([4.0, 4.0], 5.0, 128));
        assert_eq!(written.color.0[..3], theme.color("accent").unwrap().0[..3]);
        // None, and one that cannot be seen, cast nothing.
        assert_eq!(Shadow::of(&theme, None).unwrap(), None);
        assert_eq!(Shadow::of(&theme, Some(&json!({ "y": 4, "opacity": 0 }))).unwrap(), None);
        // A name the theme lacks, a key no shadow has, and an opacity past 1 say why.
        let refused = |v: Value| Shadow::of(&theme, Some(&v)).unwrap_err().to_string();
        assert!(refused(json!("dreamy")).contains("dreamy"), "{}", refused(json!("dreamy")));
        assert!(refused(json!({ "spread": 4 })).contains("spread"));
        assert!(refused(json!({ "opacity": 2 })).contains("opacity"));
    }

    #[test]
    fn a_shadow_falls_under_its_box_and_fades_in_or_out_in_a_morph() {
        let s = Shadow { offset: [0.0, 8.0], blur: 12.0, color: Color([0, 0, 0, 64]) };
        let Op::Shadow { rect, radius, blur, color } = s.op([10.0, 20.0, 100.0, 50.0], 80.0) else { panic!() };
        assert_eq!((rect, radius, blur, color), ([10.0, 28.0, 100.0, 50.0], 25.0, 12.0, Color([0, 0, 0, 64])));
        // With no blur, it is its box, a fill.
        let hard = Shadow { blur: 0.0, ..s };
        assert!(
            matches!(hard.op([0.0, 0.0, 10.0, 10.0], 2.0), Op::Fill { paint: Paint::Solid(c), .. } if c == s.color)
        );
        let half = Shadow::between(Some(s), None, 0.5).unwrap();
        assert_eq!((half.offset, half.color.0[3]), (s.offset, 32));
        let raised = Shadow { offset: [0.0, 16.0], blur: 20.0, color: Color([0, 0, 0, 96]) };
        let mid = Shadow::between(Some(s), Some(raised), 0.5).unwrap();
        assert_eq!((mid.offset, mid.blur, mid.color.0[3]), ([0.0, 12.0], 16.0, 80));
        assert_eq!(Shadow::between(None, None, 0.5), None);
    }

    #[test]
    fn an_ovals_shadow_is_a_circles_stretched_to_it() {
        let s = Shadow { offset: [0.0, 8.0], blur: 12.0, color: Color([0, 0, 0, 64]) };
        // A circle's is its own, round.
        assert!(matches!(s.oval([0.0, 0.0, 40.0, 40.0]), Op::Shadow { radius: 20.0, .. }));
        // An oval's: a circle as wide as it is tall, stretched across, falling 8 below the oval.
        let Op::Layer { transform, ops, .. } = s.oval([10.0, 20.0, 200.0, 100.0]) else { panic!() };
        assert_eq!(transform, [2.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
        let [Op::Shadow { rect, radius, blur, .. }] = ops.as_slice() else { panic!("{ops:?}") };
        assert_eq!((*rect, *radius, *blur), ([0.0, 8.0, 100.0, 100.0], 50.0, 12.0));
        // A tall one stretches down, the fall kept to the canvas's units.
        let Op::Layer { transform, ops, .. } = s.oval([0.0, 0.0, 50.0, 100.0]) else { panic!() };
        assert_eq!(transform, [1.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
        assert!(matches!(ops[0], Op::Shadow { rect: [0.0, 4.0, 50.0, 50.0], .. }), "{ops:?}");
    }
}
