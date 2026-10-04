//! `scaena export` to video painted on the GPU (PLAN 2.22): through a stand-in ffmpeg that
//! keeps the raw frames it is given, each of the GPU's frames is within SPEC §13.5's
//! tolerance of the CPU painter's. On a machine with no adapter it skips, unless
//! `SCAENA_REQUIRE_GPU` is set (CI sets it).
//!
//! One test in this file: a test that spawns while another writes the stand-in would hold
//! the script open for writing, and running it would then fail (ETXTBSY).

#![cfg(all(unix, feature = "gpu"))]

use scaena_paint::{Raster, diff};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
const SIZE: [u32; 2] = [480, 270];

/// A directory holding an `ffmpeg` that writes the frames it is given, as they come, to
/// the file it is told to write.
fn stand_in(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ffmpeg = bin.join("ffmpeg");
    std::fs::write(&ffmpeg, "#!/bin/sh\nfor out; do :; done\ncat > \"$out\"\n").unwrap();
    std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// `morph`'s cue as a video: text and shapes morphing, a group, and a shader.
fn export(bin: &Path, painter: &str, out: &Path) -> Output {
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let size = format!("{}x{}", SIZE[0], SIZE[1]);
    Command::new(env!("CARGO_BIN_EXE_scaena"))
        .args(["export", BUNDLE, "--format", "mp4", "--states", "morph", "--size", &size, "--fps", "10"])
        .args(["--painter", painter, "--json", "--out"])
        .arg(out)
        .env("PATH", path)
        .output()
        .unwrap()
}

/// The raw RGB frames the stand-in kept, as rasters.
fn frames(video: &Path) -> Vec<Raster> {
    let [width, height] = SIZE;
    let bytes = std::fs::read(video).unwrap();
    assert_eq!(bytes.len() % (width * height * 3) as usize, 0, "whole frames");
    (bytes.chunks_exact((width * height * 3) as usize))
        .map(|rgb| Raster {
            width,
            height,
            rgba: rgb.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        })
        .collect()
}

#[test]
fn the_gpu_paints_a_videos_frames_within_tolerance_of_the_cpu_painters() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("video-gpu");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bin = stand_in(&dir);
    let (cpu, gpu) = (dir.join("cpu.mp4"), dir.join("gpu.mp4"));

    let out = export(&bin, "gpu", &gpu);
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() && said.contains("no adapter") && std::env::var_os("SCAENA_REQUIRE_GPU").is_none() {
        eprintln!("skipping: {said}");
        return;
    }
    assert!(out.status.success(), "{said}");
    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(summary["painter"], "gpu", "{summary}");
    assert!(summary["adapter"].is_string(), "{summary}");

    let out = export(&bin, "cpu", &cpu);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(summary["painter"], "cpu", "{summary}");
    assert!(summary.get("adapter").is_none(), "{summary}");

    let (cpu, gpu) = (frames(&cpu), frames(&gpu));
    assert_eq!(cpu.len(), gpu.len(), "the same frames, in the same order");
    assert_eq!(Some(cpu.len() as u64), summary["frames"].as_u64());
    assert!(cpu.len() > 2, "the cue moves");
    assert!(cpu.windows(2).any(|w| w[0].rgba != w[1].rgba), "the frames are not all one");
    for (k, (cpu, gpu)) in cpu.iter().zip(&gpu).enumerate() {
        let d = diff::compare(cpu, gpu).unwrap();
        assert!(d.passes(), "frame {k}: {d}");
    }
}
