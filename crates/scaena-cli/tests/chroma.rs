//! `scaena export --chroma 444` (lift-up 20261031 ATTR-5): a video made as a master keeps every
//! pixel's color, which reaches ffmpeg's arguments, and any other chroma is refused, naming it.
//! A stand-in ffmpeg keeps the arguments it is given.
//!
//! One test in this file: a test that spawns while another writes the stand-in would hold the
//! script open for writing, and running it would then fail (ETXTBSY).

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

/// A directory holding an `ffmpeg` that writes its arguments, one a line, beside the file it is
/// told to write, and the frames it is given into it.
fn stand_in(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ffmpeg = bin.join("ffmpeg");
    let script =
        "#!/bin/sh\nfor out; do :; done\nprintf '%s\\n' \"$@\" > \"$(dirname \"$out\")/args.txt\"\ncat > \"$out\"\n";
    std::fs::write(&ffmpeg, script).unwrap();
    std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// `morph`'s cue as a small video in `format`, or what `format` refuses, with `more` after.
fn export(bin: &Path, format: &str, out: &Path, more: &[&str]) -> Output {
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let small: &[&str] = if format == "pdf" { &[] } else { &["--states", "morph", "--size", "160x90"] };
    Command::new(env!("CARGO_BIN_EXE_scaena"))
        .args(["--json", "export", BUNDLE, "--format", format])
        .args(small)
        .args(more)
        .arg("--out")
        .arg(out)
        .env("PATH", path)
        .output()
        .unwrap()
}

/// What the stand-in was given, with `--chroma` among `more`.
fn given(bin: &Path, dir: &Path, format: &str, more: &[&str]) -> Vec<String> {
    let _ = std::fs::remove_file(dir.join("args.txt"));
    let out = export(bin, format, &dir.join(format!("deck.{format}")), more);
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(dir.join("args.txt")).unwrap().lines().map(String::from).collect()
}

#[test]
fn a_master_keeps_every_pixels_color_and_any_other_chroma_is_refused() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("video-chroma");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bin = stand_in(&dir);
    let has = |args: &[String], pair: [&str; 2]| args.windows(2).any(|w| w[0] == pair[0] && w[1] == pair[1]);
    let fps = ["--fps", "2"];
    for (format, pixels, profile, default) in [
        ("mp4", "yuv444p", "high444", "yuv420p"),
        ("webm", "yuv444p", "1", "yuv420p"),
        ("prores", "yuv444p10le", "4", "yuv422p10le"),
    ] {
        let master = given(&bin, &dir, format, &[&fps[..], &["--chroma", "444"]].concat());
        assert!(has(&master, ["-pix_fmt", pixels]) && has(&master, ["-profile:v", profile]), "{format}: {master:?}");
        let usual = given(&bin, &dir, format, &fps);
        assert!(has(&usual, ["-pix_fmt", default]), "{format}: {usual:?}");
    }
    // Any other chroma, or a chroma for what is no video, is refused, naming it.
    let out = export(&bin, "mp4", &dir.join("x.mp4"), &["--chroma", "422"]);
    let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!out.status.success(), "{said}");
    assert!(said["error"]["message"].as_str().unwrap().contains("`422`"), "{said}");
    let out = export(&bin, "pdf", &dir.join("x.pdf"), &["--chroma", "444"]);
    let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(said["error"]["message"].as_str().unwrap().contains("a video's chroma"), "{said}");
}
