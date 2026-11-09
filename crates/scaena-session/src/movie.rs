//! The deck as a movie, made where the session runs (lift-up 20261031 FUNC-2): the global
//! timeline as `scaena export --format webm` plays it, each state's cue then its hold, paced by
//! the beats where a state has none ([`scaena_core::spine::paced`]), sampled `fps` times a
//! second ([`Reel`]). The session paints each frame with the CPU painter; the page encodes
//! them (WebCodecs) and writes the file. A frame that draws what the one before it drew, a hold
//! with nothing moving, is not painted again, as the CLI's video does not paint it.

use crate::{Error, Session};
use scaena_core::spine::Pace;
use scaena_ops::export::Reel;
use scaena_paint::Raster;

/// One frame of a movie: its drawing's digest, and its pixels where it draws otherwise than the
/// frame the caller had.
pub struct MovieFrame {
    pub digest: String,
    pub raster: Option<Raster>,
}

impl Session {
    /// The deck's timeline as a movie of `fps` frames a second, in the format shown: refused
    /// with why where it would run 0 ms or past a day, as the CLI's is.
    fn reel(&mut self, fps: u32) -> Result<Reel, Error> {
        let timeline = self.timeline()?;
        let paced = scaena_core::spine::paced(&self.deck, &timeline, Pace::Beats);
        Reel::of(&self.deck, &paced, None, fps).map_err(|e| Error::Ops(e.message))
    }

    /// The movie of `fps` frames a second, as JSON: `{ "frames", "fps", "durationMs" }`.
    pub fn movie(&mut self, fps: u32) -> Result<serde_json::Value, Error> {
        let reel = self.reel(fps)?;
        Ok(serde_json::json!({ "frames": reel.frames, "fps": reel.fps, "durationMs": reel.duration_ms() }))
    }

    /// Frame `k` of the movie of `fps` frames a second, `width` pixels wide: its digest, and its
    /// pixels, painted by the CPU painter, unless its digest is `before`'s, the frame the caller
    /// has already.
    pub fn movie_frame(&mut self, fps: u32, k: u64, width: u32, before: Option<&str>) -> Result<MovieFrame, Error> {
        use scaena_paint::Painter;
        let reel = self.reel(fps)?;
        if k >= reel.frames {
            return Err(Error::Ops(format!("the movie has {} frames: none is {k}", reel.frames)));
        }
        let (at, ms) = reel.at(k);
        let dl = self.frame(&reel.plays[at].state, ms)?;
        let digest = dl.digest().map_err(|e| Error::Ops(e.to_string()))?;
        if before == Some(digest.as_str()) {
            return Ok(MovieFrame { digest, raster: None });
        }
        let scale = width as f32 / dl.viewport[0];
        let raster = self.painter.paint(&dl, &self.store, scale)?;
        Ok(MovieFrame { digest, raster: Some(raster) })
    }
}
