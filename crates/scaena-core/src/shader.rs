//! What a shader op draws (SPEC §3.8). Each kind's CPU reference and WGSL twin are in
//! `scaena-pixels`, the work a painter does per pixel, which the browser's modules build for
//! speed while the rest of core is built for size (PLAN 2.98); this module is how everything
//! reaches them: a [`Job`] made from a display list's shader op ([`job`]), what it is made
//! from as bytes ([`Spec`]), and what a document's params read as. Painters depend only on
//! core.

pub use scaena_pixels::shader::{
    BAND, Job, MAX_STOPS, ShaderError, bands, cores, gradient, grain, mesh, noise, particles,
};

use crate::displaylist::{Op, ShaderKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The job for shader op `op` drawn through `device` (canvas units to device pixels,
/// `[a, b, c, d, e, f]` as in the display list) into a raster `size` pixels across.
/// `None` when the op covers no pixel of the raster.
pub fn job(op: &Op, device: [f64; 6], size: [u32; 2]) -> Result<Option<Job>, ShaderError> {
    let Op::Shader { kind, seed, t, rect, palette, params } = op else {
        return Ok(None);
    };
    let (seed, t, rect) = (*seed, *t, *rect);
    let palette: Vec<scaena_pixels::shader::Color> =
        palette.iter().map(|c| scaena_pixels::shader::Color(c.0)).collect();
    let palette = palette.as_slice();
    Ok(match kind {
        ShaderKind::Mesh => {
            let params = mesh::Params::from_map(params)?;
            mesh::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Mesh)
        }
        ShaderKind::Gradient => {
            let params = gradient::Params::from_map(params)?;
            gradient::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Gradient)
        }
        ShaderKind::Noise => {
            let params = noise::Params::from_map(params)?;
            noise::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Noise)
        }
        ShaderKind::Grain => {
            let params = grain::Params::from_map(params)?;
            grain::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Grain)
        }
        ShaderKind::Particles => {
            let params = particles::Params::from_map(params)?;
            particles::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Particles)
        }
    })
}

/// Whether `params` are ones a `kind` shader takes, each in its range (SPEC §3.8). The
/// engine asks when it resolves a node, so a bad param is the document's error.
pub fn check(kind: ShaderKind, params: &BTreeMap<String, f32>) -> Result<(), ShaderError> {
    match kind {
        ShaderKind::Mesh => mesh::Params::from_map(params).map(drop),
        ShaderKind::Gradient => gradient::Params::from_map(params).map(drop),
        ShaderKind::Noise => noise::Params::from_map(params).map(drop),
        ShaderKind::Grain => grain::Params::from_map(params).map(drop),
        ShaderKind::Particles => particles::Params::from_map(params).map(drop),
    }
}

/// The number a shader op carries for a param a document writes as a name: a gradient's
/// `shape`. Shader ops carry numbers only (SPEC §6).
pub fn named(kind: ShaderKind, param: &str, name: &str) -> Result<f32, ShaderError> {
    let bad = |problem: String| ShaderError::Param { name: param.to_string(), problem };
    match (kind, param) {
        (ShaderKind::Gradient, "shape") => gradient::Shape::named(name)
            .map(|s| s as u32 as f32)
            .ok_or_else(|| bad(format!("`{name}`: expected linear, radial, or conic"))),
        _ => Err(bad(format!("`{name}`: expected a number"))),
    }
}

/// Whether `param` of `kind` moves through the values between when a shader morphs
/// from one state to the next (SPEC §3.8). A name (a gradient's `shape`) and a count
/// (mesh `points`, noise `octaves`, particles `count`) have nothing between, and a rate
/// (`speed`, grain `fps`) on the global clock would race its phase through (b − a)·t
/// on the way (SPEC §3.9): two shaders that differ in one of these cross-fade.
pub fn interpolates(kind: ShaderKind, param: &str) -> bool {
    !matches!(
        (kind, param),
        (ShaderKind::Mesh, "points")
            | (ShaderKind::Gradient, "shape" | "speed")
            | (ShaderKind::Noise, "octaves" | "speed")
            | (ShaderKind::Grain, "fps")
            | (ShaderKind::Particles, "count" | "speed")
    )
}

/// The value `param` of `kind` takes when a document leaves it out, as an op carries it;
/// `None` for a param the kind does not take.
pub fn default(kind: ShaderKind, param: &str) -> Option<f32> {
    let values: Vec<(&str, f32)> = match kind {
        ShaderKind::Mesh => {
            let p = mesh::Params::default();
            vec![("points", p.points as f32), ("drift", p.drift), ("softness", p.softness), ("grain", p.grain)]
        }
        ShaderKind::Gradient => {
            let p = gradient::Params::default();
            vec![
                ("shape", p.shape as u32 as f32),
                ("angle", p.angle),
                ("x", p.x),
                ("y", p.y),
                ("radius", p.radius),
                ("speed", p.speed),
                ("grain", p.grain),
            ]
        }
        ShaderKind::Noise => {
            let p = noise::Params::default();
            vec![
                ("scale", p.scale),
                ("octaves", p.octaves as f32),
                ("speed", p.speed),
                ("contrast", p.contrast),
                ("grain", p.grain),
            ]
        }
        ShaderKind::Grain => {
            let p = grain::Params::default();
            vec![("amount", p.amount), ("fps", p.fps)]
        }
        ShaderKind::Particles => {
            let p = particles::Params::default();
            vec![("count", p.count as f32), ("size", p.size), ("speed", p.speed), ("softness", p.softness)]
        }
    };
    values.into_iter().find(|(name, _)| *name == param).map(|(_, v)| v)
}

/// What a [`Job`] is made from, [`job`]'s arguments: a shader op, the transform from
/// canvas units to device pixels it is drawn through, and the size of the raster. As bytes
/// it crosses to another worker, which makes the same job again: the browser, whose
/// WebAssembly has no threads, works a shader's [`bands`] out on workers of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    pub op: Op,
    pub device: [f64; 6],
    pub size: [u32; 2],
}

impl Spec {
    /// The job: [`job`] on the spec's arguments.
    pub fn job(&self) -> Result<Option<Job>, ShaderError> {
        job(&self.op, self.device, self.size)
    }

    /// The spec as postcard bytes, each number as its bits: the job made from them is
    /// this spec's, bit for bit.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ShaderError> {
        postcard::to_allocvec(self).map_err(|e| ShaderError::Spec(e.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Spec, ShaderError> {
        postcard::from_bytes(bytes).map_err(|e| ShaderError::Spec(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::displaylist::Color;

    #[test]
    fn defaults_are_what_each_kind_takes_when_a_param_is_left_out() {
        let kinds = [
            (ShaderKind::Mesh, &["points", "drift", "softness", "grain"][..]),
            (ShaderKind::Gradient, &["shape", "angle", "x", "y", "radius", "speed", "grain"]),
            (ShaderKind::Noise, &["scale", "octaves", "speed", "contrast", "grain"]),
            (ShaderKind::Grain, &["amount", "fps"]),
            (ShaderKind::Particles, &["count", "size", "speed", "softness"]),
        ];
        for (kind, params) in kinds {
            let all: BTreeMap<String, f32> = params
                .iter()
                .map(|p| (p.to_string(), default(kind, p).unwrap_or_else(|| panic!("{kind:?} {p}"))))
                .collect();
            // Written out, the defaults are a valid op, and draw what leaving them out draws.
            check(kind, &all).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            let rect = [0.0, 0.0, 8.0, 8.0];
            let op = |params: BTreeMap<String, f32>| Op::Shader {
                kind,
                seed: 3,
                t: 1.5,
                rect,
                palette: vec![Color([230, 50, 25, 255]), Color([25, 75, 200, 255])],
                params,
            };
            let draw = |op: &Op| job(op, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [8, 8]).unwrap().unwrap().render();
            assert_eq!(draw(&op(all)), draw(&op(BTreeMap::new())), "{kind:?}");
            assert_eq!(default(kind, "nonsense"), None);
        }
        assert!(interpolates(ShaderKind::Gradient, "angle") && !interpolates(ShaderKind::Gradient, "speed"));
    }

    /// `render_on` is `render` on any number of threads, in bands that do not split the box
    /// evenly; and `render_rows` is `render`'s rows from anywhere. For every kind.
    #[test]
    fn rows_in_bands_are_the_render() {
        let palette = vec![Color([20, 10, 40, 255]), Color([255, 240, 220, 200]), Color([90, 180, 120, 255])];
        let (w, h) = (150_usize, 515_usize);
        for kind in
            [ShaderKind::Mesh, ShaderKind::Gradient, ShaderKind::Noise, ShaderKind::Grain, ShaderKind::Particles]
        {
            let rect = [0.0, 0.0, w as f32, h as f32];
            let op = Op::Shader { kind, seed: 3, t: 1.25, rect, palette: palette.clone(), params: BTreeMap::new() };
            let job = job(&op, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [w as u32, h as u32]).unwrap().unwrap();
            let whole = job.render();
            for threads in 1..=9 {
                assert!(job.render_on(threads) == whole, "{kind:?} on {threads} threads");
            }
            let row = w * 4;
            for (first, n) in [(0, 1), (7, 13), (h - 13, 13), (BAND, BAND)] {
                let mut out = vec![0; n * row];
                job.render_rows(first as u32, &mut out);
                assert!(out == whole[first * row..(first + n) * row], "{kind:?}, rows {first} to {}", first + n);
            }
            // Made again from its spec's bytes, the job is the same job, and its bands, each
            // worked out apart, are the render.
            let spec = Spec { op: op.clone(), device: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], size: [w as u32, h as u32] };
            let again = Spec::from_bytes(&spec.to_bytes().unwrap()).unwrap().job().unwrap().unwrap();
            assert_eq!(again, job, "{kind:?}");
            let mut banded = Vec::new();
            for [first, rows] in bands(h as u32, 4) {
                let mut out = vec![0; rows as usize * row];
                again.render_rows(first, &mut out);
                banded.extend(out);
            }
            assert!(banded == whole, "{kind:?} in bands");
        }
    }

    /// Bytes that are not a spec are an error that says so, never a panic.
    #[test]
    fn a_damaged_spec_is_an_error() {
        let op = Op::Shader {
            kind: ShaderKind::Noise,
            seed: u64::MAX,
            t: 3.5,
            rect: [0.0, 0.0, 64.0, 64.0],
            palette: vec![Color([1, 2, 3, 255])],
            params: BTreeMap::from([("octaves".to_string(), 3.0)]),
        };
        let bytes = Spec { op, device: [0.5, 0.0, 0.0, 0.5, 0.25, 0.0], size: [32, 32] }.to_bytes().unwrap();
        for cut in 0..bytes.len() {
            assert!(matches!(Spec::from_bytes(&bytes[..cut]), Err(ShaderError::Spec(_))), "cut at {cut}");
        }
        assert!(matches!(Spec::from_bytes(&[0xff; 9]), Err(ShaderError::Spec(_))));
    }
}
