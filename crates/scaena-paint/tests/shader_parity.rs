//! Shader parity (SPEC §14, PLAN 0.11): each kind's CPU reference against its WGSL twin
//! on this machine's GPU, per seed, at several times, parameters, and device
//! transforms. Judged by the SPEC §13.5 metric over every pixel: a shader has no
//! anti-aliased edges to mask. Without an adapter the test is skipped and says so,
//! unless `SCAENA_REQUIRE_GPU=1` (CI sets it).
#![cfg(feature = "gpu")]

use scaena_core::displaylist::{Color, Op, ShaderKind};
use scaena_core::shader::Job;
use scaena_paint::gpu::GpuPainter;
use scaena_paint::{Raster, diff};
use std::collections::BTreeMap;

fn painter() -> Option<GpuPainter> {
    match GpuPainter::new() {
        Ok(p) => Some(p),
        Err(e) if std::env::var_os("SCAENA_REQUIRE_GPU").is_none() => {
            eprintln!("skipping: {e} (set SCAENA_REQUIRE_GPU=1 to fail instead)");
            None
        }
        Err(e) => panic!("SCAENA_REQUIRE_GPU is set: {e}"),
    }
}

fn hex(s: &str) -> Color {
    Color::from_hex(s).unwrap()
}

fn mesh(seed: u64, t: f32, rect: [f32; 4], palette: &[Color], params: &[(&str, f32)]) -> Op {
    Op::Shader {
        kind: ShaderKind::Mesh,
        seed,
        t,
        rect,
        palette: palette.to_vec(),
        params: params.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn mesh_cpu_reference_and_wgsl_agree() {
    let Some(mut gpu) = painter() else { return };
    let info = gpu.adapter();
    println!("gpu: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
    let torture = ["#0F766E", "#4338CA", "#C2410C", "#F5C451"].map(hex);
    let dusk = ["#1B1430", "#3A1F4F", "#B0452C", "#FF6A3D", "#F2F0E9"].map(hex);
    let glass = ["#FFFFFF00", "#0F766ECC", "#C2410C80"].map(hex);
    let scale2 = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    let (c, s) = (0.866_025_403_784_438_6, 0.5);
    let rotated = [c, s, -s, c, 400.0, -100.0];
    let cases: Vec<(&str, Op, [f64; 6], [u32; 2])> = vec![
        (
            "torture deck, 1080p",
            mesh(
                7,
                0.84,
                [0.0, 0.0, 1920.0, 1080.0],
                &torture,
                &[("points", 5.0), ("drift", 0.12), ("softness", 0.85), ("grain", 0.035)],
            ),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1920, 1080],
        ),
        (
            "seed 0, t 0",
            mesh(0, 0.0, [0.0, 0.0, 640.0, 360.0], &torture, &[]),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [640, 360],
        ),
        (
            "an hour in",
            mesh(42, 3600.0, [0.0, 0.0, 640.0, 360.0], &dusk, &[("drift", 0.5)]),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [640, 360],
        ),
        (
            "16 tight points",
            mesh(
                1 << 40 | 3,
                12.5,
                [0.0, 0.0, 400.0, 400.0],
                &dusk,
                &[("points", 16.0), ("softness", 0.05), ("grain", 0.0)],
            ),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [400, 400],
        ),
        (
            "heavy grain, portrait",
            mesh(9, 2.0, [0.0, 0.0, 270.0, 480.0], &dusk, &[("softness", 1.0), ("grain", 0.25)]),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [270, 480],
        ),
        ("translucent palette at 2x", mesh(11, 1.0, [10.0, 5.0, 200.0, 100.0], &glass, &[]), scale2, [440, 220]),
        ("rotated, clipped by the raster", mesh(5, 4.0, [0.0, 0.0, 600.0, 300.0], &torture, &[]), rotated, [640, 480]),
    ];
    let mut failures = Vec::new();
    for (name, op, device, size) in &cases {
        let job = Job::new(op, *device, *size).unwrap().expect("covers pixels");
        let [_, _, w, h] = job.bbox();
        let cpu = Raster { width: w, height: h, rgba: job.render() };
        let gpu = Raster { width: w, height: h, rgba: gpu.shader_pixels(&job).unwrap() };
        let mut d = diff::compare(&cpu, &gpu).unwrap();
        // Every pixel counts: recount ΔE without the edge mask.
        let (mut over, mut max) = (0, 0.0_f32);
        for (p, q) in cpu.rgba.as_chunks::<4>().0.iter().zip(gpu.rgba.as_chunks::<4>().0) {
            let e = diff::delta_e(*p, *q);
            over += usize::from(e > diff::MAX_DELTA_E);
            max = max.max(e);
        }
        (d.compared, d.over, d.max_delta_e) = (d.pixels, over, max);
        println!("{name:<32} {w}×{h}: {d}");
        if !d.passes() {
            failures.push(format!("{name}: {d}"));
        }
    }
    assert!(failures.is_empty(), "CPU reference and WGSL disagree:\n{}", failures.join("\n"));
}
