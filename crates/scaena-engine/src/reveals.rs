//! Slide transitions that show the slide entered through shapes that grow (PLAN 2.149, SPEC §3.9):
//! circles, diamonds, each in a wave, and a clock's hand. Each is a vector mask, a [`Path`] the
//! slide entered is clipped to, made from the transition's progress and the canvas's size alone,
//! so every painter, the SVG, and a video draw it exactly, and a frame lays nothing out.
//!
//! The geometry is ported from Inferno's transition shaders, `Circle.metal`, `CircleWave.metal`,
//! `Diamond.metal`, `DiamondWave.metal`, and `Radial.metal`
//! (<https://github.com/twostraws/Inferno>, `Sources/Inferno/Shaders/Transition/`), which are
//! themselves ported from gl-transitions' PolkaDotsCurtain (bobylito) and Radial (Xaychru / gre).
//! Inferno decides each pixel; here the same tests are solved for each shape's size:
//!
//! - The canvas is cut into square cells, nine across its shorter side, from its top-left corner,
//!   as Inferno's `fract(position / size)` cuts it.
//! - Circle: Inferno shows a pixel whose distance from its cell's middle, in cells, is under the
//!   progress, so every circle has the radius `p` cells; here that is scaled by √½, the distance
//!   to a cell's corner, so the last pixel shows as the transition ends rather than seven tenths of
//!   the way. Diamond: the same by the Manhattan distance, whose corner is 1, as Inferno has it.
//! - The waves: Inferno shows a pixel where its distance plus `u + v`, its place from the top-left
//!   corner, is under `3p`. Here the wave goes the transition's `direction`, so `u + v` is twice
//!   the cell's middle's place along that way, 0 at the edge it starts from and 2 at the far one,
//!   and each cell's shape is whole: its size `3p − 2k` cells, at most what covers the cell.
//! - Radial: Inferno sweeps clockwise from straight up about the canvas's middle. Here the sweep
//!   is a wedge of `p` of a turn, the hard edge Inferno's `smoothstep` softens, starting straight
//!   up for `left`, the default, and a quarter turn on for each other way (see [`radial`]).
//!
//! Inferno's license, which `LICENSE-MIT-inferno` beside this crate's manifest carries whole:
//!
//! ```text
//! MIT License
//!
//! Copyright (c) 2023 Paul Hudson and other authors.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//! ```

use crate::charts::{arc, circle};
use scaena_core::displaylist::{Path, PathEl};

/// How many cells across the canvas's shorter side.
const CELLS: f32 = 9.0;
/// The distance from a cell's middle to its corner, in cells.
const HALF_DIAGONAL: f32 = core::f32::consts::FRAC_1_SQRT_2;

/// The shape each cell grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Circle,
    Diamond,
}

/// The cells' shapes `p` of the way through, on a canvas `size` big, as one path: each a circle or
/// a diamond about its cell's middle, clockwise, so where they overlap the nonzero rule fills them
/// as one. With `wave`, the way the wave goes as a unit vector, each cell's shape starts later the
/// further along that way its middle lies.
pub fn cells(cell: Cell, wave: Option<[f32; 2]>, p: f32, [w, h]: [f32; 2]) -> Path {
    let s = w.min(h) / CELLS;
    let p = p.clamp(0.0, 1.0);
    let mut els = Vec::new();
    if s.is_nan() || s <= 0.0 {
        return Path(els);
    }
    let (cols, rows) = ((w / s).ceil() as usize, (h / s).ceil() as usize);
    // What covers a cell whole, in cells: a circle out to its corners, a diamond out to 1.
    let whole = match cell {
        Cell::Circle => HALF_DIAGONAL,
        Cell::Diamond => 1.0,
    };
    for row in 0..rows {
        for col in 0..cols {
            let middle = [(col as f32 + 0.5) * s, (row as f32 + 0.5) * s];
            let size = match wave {
                // Inferno's `d + u + v < 3p`, `u + v` twice the place along the wave.
                Some(way) => (3.0 * p - 2.0 * along(way, middle, [w, h])).min(whole),
                // Inferno's `d < p`, a circle's scaled to reach its corners as `p` reaches 1.
                None => p * whole,
            };
            if size <= 0.0 {
                continue;
            }
            let r = size * s;
            match cell {
                Cell::Circle => els.extend(circle(middle, r).0),
                Cell::Diamond => {
                    let [x, y] = middle;
                    els.extend([
                        PathEl::MoveTo([x, y - r]),
                        PathEl::LineTo([x + r, y]),
                        PathEl::LineTo([x, y + r]),
                        PathEl::LineTo([x - r, y]),
                        PathEl::Close,
                    ]);
                }
            }
        }
    }
    Path(els)
}

/// How far along the way `way` a point lies on a canvas `size` big: 0 at the edge it starts from,
/// 1 at the far one. A wave `left` starts at the right edge.
fn along(way: [f32; 2], [x, y]: [f32; 2], [w, h]: [f32; 2]) -> f32 {
    match way {
        [dx, _] if dx < 0.0 => 1.0 - x / w,
        [dx, _] if dx > 0.0 => x / w,
        [_, dy] if dy < 0.0 => 1.0 - y / h,
        _ => y / h,
    }
}

/// A clock's hand `p` of a turn round from where it starts, clockwise about the canvas's middle,
/// the wedge it has swept reaching past the canvas's corners. It starts so the slide entered shows
/// first on the side a push brings it from: straight up for `left`, the default, so the right half
/// shows first, as Inferno's does; at three o'clock for `up`, at six for `right`, and at nine for
/// `down`.
pub fn radial(way: [f32; 2], p: f32, [w, h]: [f32; 2]) -> Path {
    let p = p.clamp(0.0, 1.0);
    if p <= 0.0 {
        return Path(Vec::new());
    }
    let start = match way {
        [dx, _] if dx > 0.0 => 0.5,
        [_, dy] if dy < 0.0 => 0.25,
        [_, dy] if dy > 0.0 => 0.75,
        _ => 0.0,
    };
    // Past the corners by a margin, so a cubic's chord between its ends never cuts one off.
    let reach = libm::hypotf(w, h) / 2.0 * 1.1;
    arc([w / 2.0, h / 2.0], 0.0, reach, start, start + p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: [f32; 2] = [1920.0, 1080.0];
    const LEFT: [f32; 2] = [-1.0, 0.0];

    /// Whether `point` is inside `path` by the nonzero rule, its curves flattened finely.
    fn inside(path: &Path, [px, py]: [f32; 2]) -> bool {
        let mut winding = 0;
        let (mut at, mut first) = ([0.0f32; 2], [0.0f32; 2]);
        let mut edge = |a: [f32; 2], b: [f32; 2]| {
            if (a[1] <= py) != (b[1] <= py) {
                let x = a[0] + (py - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if x > px {
                    winding += if b[1] > a[1] { 1 } else { -1 };
                }
            }
        };
        for el in &path.0 {
            match *el {
                PathEl::MoveTo(p) => {
                    (at, first) = (p, p);
                }
                PathEl::LineTo(p) => {
                    edge(at, p);
                    at = p;
                }
                PathEl::QuadTo(c, p) => {
                    edge(at, c);
                    edge(c, p);
                    at = p;
                }
                PathEl::CurveTo(c0, c1, p) => {
                    let n = 32;
                    let mut last = at;
                    for i in 1..=n {
                        let t = i as f32 / n as f32;
                        let u = 1.0 - t;
                        let q = |k: usize| {
                            u * u * u * at[k] + 3.0 * u * u * t * c0[k] + 3.0 * u * t * t * c1[k] + t * t * t * p[k]
                        };
                        let next = [q(0), q(1)];
                        edge(last, next);
                        last = next;
                    }
                    at = p;
                }
                PathEl::Close => {
                    edge(at, first);
                    at = first;
                }
            }
        }
        winding != 0
    }

    /// Points spread over the canvas, its corners and edges among them.
    fn samples() -> Vec<[f32; 2]> {
        let mut out = Vec::new();
        for i in 0..=48 {
            for j in 0..=27 {
                out.push([(i as f32 * 40.0).clamp(0.5, SIZE[0] - 0.5), (j as f32 * 40.0).clamp(0.5, SIZE[1] - 0.5)]);
            }
        }
        out
    }

    /// Each mask `p` of the way through: every style, each wave every way.
    fn masks(p: f32) -> Vec<(String, Path)> {
        let ways = [LEFT, [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]];
        let mut out = vec![
            ("circle".to_string(), cells(Cell::Circle, None, p, SIZE)),
            ("diamond".to_string(), cells(Cell::Diamond, None, p, SIZE)),
        ];
        for way in ways {
            out.push((format!("circleWave {way:?}"), cells(Cell::Circle, Some(way), p, SIZE)));
            out.push((format!("diamondWave {way:?}"), cells(Cell::Diamond, Some(way), p, SIZE)));
            out.push((format!("radial {way:?}"), radial(way, p, SIZE)));
        }
        out
    }

    #[test]
    fn at_the_start_nothing_shows_and_at_the_end_everything_does() {
        for (name, mask) in masks(0.0) {
            assert!(mask.0.is_empty(), "{name}: no shape at the start, the slide left alone");
        }
        for (name, mask) in masks(1.0) {
            let missed: Vec<_> = samples().into_iter().filter(|q| !inside(&mask, *q)).collect();
            assert!(
                missed.is_empty(),
                "{name}: the slide entered alone at the end, but {missed:?} show the slide left"
            );
        }
    }

    #[test]
    fn halfway_each_shows_some_of_each_slide() {
        for (name, mask) in masks(0.5) {
            let shown = samples().into_iter().filter(|q| inside(&mask, *q)).count();
            assert!(shown > 0 && shown < samples().len(), "{name}: {shown} of {}", samples().len());
        }
    }

    #[test]
    fn a_wave_starts_at_the_edge_it_comes_from() {
        // A quarter of the way, a wave going left has opened cells at the right edge alone, and one
        // going down at the top alone; the plain circles have opened every cell alike.
        let right = [SIZE[0] - 60.0, 540.0];
        let left = [60.0, 540.0];
        let wave = cells(Cell::Circle, Some(LEFT), 0.25, SIZE);
        assert!(inside(&wave, right) && !inside(&wave, left));
        let down = cells(Cell::Diamond, Some([0.0, 1.0]), 0.25, SIZE);
        assert!(inside(&down, [960.0, 60.0]) && !inside(&down, [960.0, SIZE[1] - 60.0]));
        let all = cells(Cell::Circle, None, 0.25, SIZE);
        assert!(inside(&all, right) && inside(&all, left));
    }

    #[test]
    fn the_hand_starts_where_its_way_says_and_turns_clockwise() {
        // An eighth of a turn from straight up: just right of twelve o'clock shows, just left does not.
        let (w, h) = (SIZE[0], SIZE[1]);
        let up_right = [w / 2.0 + 40.0, 40.0];
        let up_left = [w / 2.0 - 40.0, 40.0];
        let hand = radial(LEFT, 0.125, SIZE);
        assert!(inside(&hand, up_right) && !inside(&hand, up_left));
        // From three o'clock, for `up`: just below it shows.
        let hand = radial([0.0, -1.0], 0.125, SIZE);
        assert!(inside(&hand, [w - 40.0, h / 2.0 + 40.0]) && !inside(&hand, [w - 40.0, h / 2.0 - 40.0]));
        // From six, for `right`; from nine, for `down`.
        let hand = radial([1.0, 0.0], 0.125, SIZE);
        assert!(inside(&hand, [w / 2.0 - 40.0, h - 40.0]) && !inside(&hand, [w / 2.0 + 40.0, h - 40.0]));
        let hand = radial([0.0, 1.0], 0.125, SIZE);
        assert!(inside(&hand, [40.0, h / 2.0 - 40.0]) && !inside(&hand, [40.0, h / 2.0 + 40.0]));
    }
}
