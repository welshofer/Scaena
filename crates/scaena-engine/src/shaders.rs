//! Shader nodes (SPEC §3.8). Every kind has a CPU reference implementation here
//! and a WGSL twin under `shaders/wgsl/`; the GPU is an optimization and parity is
//! tested per kind (PLAN 0.11, 1.10). No arbitrary shader source, ever.

/// Parameters common to every kind.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderInput {
    pub seed: u64,
    pub t: f64,
    pub rect: [f64; 4],
    pub palette: Vec<[f32; 4]>,
}

/// Evaluate a kind at normalized `(u, v)` on the CPU. Phase 0 implements `mesh`.
pub fn eval(_kind: &str, _input: &ShaderInput, _u: f32, _v: f32) -> [f32; 4] {
    unimplemented!("shaders::eval — PLAN 0.11")
}
