//! Every format at once (PLAN 2.62): the state shown in each of the deck's formats beside the
//! canvas, painted as it plays. Each format keeps what it laid out, the state at rest and the
//! cue it samples, so its frames lay nothing out (SPEC §5). The canvas showing another format
//! keeps them; the deck, its theme, or its files changing lets them go, as it does the canvas's.

use crate::{Error, Session};
use scaena_core::timeline::Timeline;
use scaena_engine::sample::{Scene, Transition};
use scaena_engine::{EngineError, FrameRequest, project};

/// What a format painted beside the canvas keeps from one frame to the next.
#[derive(Default)]
pub(crate) struct Beside {
    /// The deck's states end to end, in the format.
    timeline: Option<Timeline>,
    /// The state last painted at rest, laid out in the format.
    rest: Option<(String, Scene)>,
    /// The cue last sampled, laid out in the format.
    transition: Option<(String, Transition)>,
    /// A painter of its own, which keeps its render context at the format's size.
    painter: scaena_paint::cpu::CpuPainter,
}

impl Session {
    /// `state` `t_ms` into its cue in `format`, one of the deck's `formats` or its own canvas
    /// (`None`), painted by the format's own CPU painter `height` pixels high, the width keeping
    /// the format's aspect: what the canvas shows there (PLAN 2.62), so formats side by side
    /// share a height. Past the cue's span, at
    /// rest, its shaders `t_ms` into the deck's timeline as the canvas's are. Laid out once for
    /// each state and format, and kept while the canvas shows another format; neither a drag
    /// nor a patch previewed draws in it.
    pub fn pixels_in(
        &mut self,
        format: Option<&str>,
        state: &str,
        t_ms: f64,
        height: u32,
    ) -> Result<scaena_paint::Raster, Error> {
        use scaena_paint::Painter;
        self.build()?;
        let Session { besides, engine, deck, theme, data, store, .. } = self;
        let engine = engine.as_mut().expect("built above");
        let beside = besides.entry(format.map(str::to_string)).or_default();
        if beside.timeline.is_none() {
            let (deck, theme) = project(deck, theme, format)?;
            beside.timeline = Some(engine.timeline(&deck, &theme, data)?);
        }
        let timeline = beside.timeline.as_ref().expect("worked out above");
        let slot = timeline.slot(state).ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
        let (start, span) = (slot.start, slot.span);
        let list = if t_ms.is_nan() || t_ms >= span {
            if beside.rest.as_ref().is_none_or(|(s, _)| s != state) {
                let req = FrameRequest { deck, theme, data, state, t_ms: f64::INFINITY, format };
                beside.rest = Some((state.to_string(), engine.at_rest(&req)?));
            }
            // Its shaders at the time it comes to rest, or `t_ms` into its hold.
            let time = (start + if t_ms.is_finite() { t_ms } else { span }) / 1000.0;
            beside.rest.as_ref().expect("laid out above").1.draw_at(time)
        } else {
            if beside.transition.as_ref().is_none_or(|(s, _)| s != state) {
                let (deck, theme) = project(deck, theme, format)?;
                beside.transition = Some((state.to_string(), engine.transition(&deck, &theme, data, state)?));
            }
            beside.transition.as_ref().expect("laid out above").1.frame(t_ms)
        };
        let scale = height as f32 / list.viewport[1];
        Ok(beside.painter.paint(&list, store, scale)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use crate::store::tests::revenue;

    /// Each format beside the canvas (PLAN 2.62): painted as the canvas paints it in that format,
    /// at rest and inside the cue, laid out once and kept while the canvas changes format, and
    /// let go when the deck changes.
    #[test]
    fn each_format_is_painted_as_the_canvas_paints_it() {
        let mut s = Session::open(revenue()).unwrap();
        let span = s.duration("revenue").unwrap();
        assert!(span > 0.0, "the revenue state has a cue");
        for format in [None, Some("9:16")] {
            // An eighth of the canvas, as the canvas paints it at that scale.
            s.set_format(format).unwrap();
            let [w, h] = s.canvas_size().unwrap().map(|n| (n / 8.0) as u32);
            s.set_format(None).unwrap();
            for t in [span / 2.0, f64::INFINITY] {
                let beside = s.pixels_in(format, "revenue", t, h).unwrap();
                s.set_format(format).unwrap();
                let shown = s.pixels("revenue", t, w).unwrap();
                s.set_format(None).unwrap();
                assert_eq!((beside.width, beside.height), (w, h), "{format:?} at {t}");
                assert_eq!((shown.width, shown.height), (w, h), "{format:?} at {t}");
                assert!(beside.rgba == shown.rgba, "{format:?} at {t}: the bytes the canvas paints");
            }
        }
        // Kept while the canvas shows another format: the cue and the state at rest, each laid
        // out once.
        s.set_format(Some("9:16")).unwrap();
        let tall = &s.besides[&Some("9:16".to_string())];
        assert!(tall.rest.as_ref().is_some_and(|(state, _)| state == "revenue"));
        assert!(tall.transition.as_ref().is_some_and(|(state, _)| state == "revenue"));
        // A format the deck does not list is refused, as the canvas refuses it.
        assert!(s.pixels_in(Some("4:5"), "revenue", f64::INFINITY, 135).is_err());
        // The deck changed: what each format laid out is let go.
        let deck = s.deck.clone();
        s.set_deck(deck);
        assert!(s.besides.is_empty());
    }
}
