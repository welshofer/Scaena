//! Video export (PLAN 1.21): the global timeline sampled at a frame rate, encoded by
//! ffmpeg. Each test needs ffmpeg on the PATH and is skipped without it, unless
//! SCAENA_REQUIRE_FFMPEG is set, as CI sets it where it installs ffmpeg.

use scaena_ops::export::{Exported, Request, export};
use scaena_ops::render::{Request as Render, render};
use scaena_paint::Raster;
use std::path::{Path, PathBuf};
use std::process::Command;

const REVENUE: &str = "../../docs/examples/revenue.deck.json";

fn ffmpeg() -> bool {
    let found = Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success());
    if !found {
        assert!(
            std::env::var_os("SCAENA_REQUIRE_FFMPEG").is_none(),
            "SCAENA_REQUIRE_FFMPEG is set, and no ffmpeg runs"
        );
        eprintln!("skipped: no ffmpeg on the PATH");
    }
    found
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("video");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

fn exported(format: &str, states: Option<&[&str]>, size: &str, fps: u32, out: &Path) -> Exported {
    let req = Request {
        format: format.into(),
        states: states.map(|s| s.iter().map(|s| s.to_string()).collect()),
        out: Some(out.to_path_buf()),
        size: Some(size.into()),
        fps: Some(fps),
        audio: None,
    };
    export(&scaena_ops::open(Path::new(REVENUE)).unwrap(), &req).unwrap()
}

/// Every frame of a video, decoded to RGB.
fn decode(video: &Path, [w, h]: [u32; 2]) -> Vec<Vec<u8>> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(video)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    out.stdout.chunks((w * h * 3) as usize).map(<[u8]>::to_vec).collect()
}

/// How far a decoded frame is from the CPU painter's frame of `state` at `t`: the mean
/// difference per channel, out of 255.
fn distance(frame: &[u8], state: &str, t: f64, size: &str) -> f64 {
    let req = Render { state: state.into(), t: Some(t), size: Some(size.into()), ..Render::default() };
    let painted = Raster::from_png(&render(Path::new(REVENUE), &req).unwrap().png).unwrap();
    let rgb = painted.rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, _]| [r, g, b]);
    let total: u64 = rgb.zip(frame).map(|(a, &b)| u64::from(a.abs_diff(b))).sum();
    total as f64 / frame.len() as f64
}

#[test]
fn a_video_plays_each_state_then_its_hold() {
    if !ffmpeg() {
        return;
    }
    let out = scratch("revenue.mp4");
    let video = exported("mp4", None, "320x180", 10, &out);
    // intro: 660 ms of motion, then a 4 s hold; revenue: 1440 + 6000; mix: 800 + 6000;
    // close: 0 + 3000. 21.9 s at 10 frames a second.
    let starts: Vec<f64> = video.timeline.as_ref().unwrap().iter().map(|p| p.start).collect();
    assert_eq!(starts, [0.0, 4660.0, 12100.0, 18900.0]);
    assert_eq!((video.frames, video.duration_ms, video.size), (Some(219), Some(21900.0), Some([320, 180])));
    let frames = decode(&out, [320, 180]);
    assert_eq!(frames.len(), 219);
    // A frame shows the moment `render` draws, as near as H.264 keeps it: in a cue, at
    // rest in a hold, and the last. In a cue, nearer that moment than 100 ms either side.
    let moments = [
        (3, "intro", 300.0, true),
        (30, "intro", 3000.0, false),
        (50, "revenue", 340.0, true),
        (150, "mix", 2900.0, false),
        (218, "close", 2900.0, false),
    ];
    for (k, state, t, moving) in moments {
        let here = distance(&frames[k], state, t, "320x180");
        assert!(here < 2.0, "frame {k} is {here:.2} from `{state}` at {t} ms");
        for near in [t - 100.0, t + 100.0].into_iter().filter(|_| moving) {
            let there = distance(&frames[k], state, near, "320x180");
            assert!(there > here, "frame {k} is nearer `{state}` at {near} ms ({there:.2}) than at {t} ({here:.2})");
        }
    }
}

#[test]
fn webm_and_prores_play_the_states_asked_for() {
    if !ffmpeg() {
        return;
    }
    for (format, file, codec) in [("webm", "close.webm", &b"V_VP9"[..]), ("prores", "close.mov", &b"apch"[..])] {
        let out = scratch(file);
        let video = exported(format, Some(&["close", "intro"]), "320x180", 5, &out);
        // close's 3 s hold, then intro's 4.66 s.
        let starts: Vec<f64> = video.timeline.as_ref().unwrap().iter().map(|p| p.start).collect();
        assert_eq!((starts.as_slice(), video.frames), (&[0.0, 3000.0][..], Some(39)), "{format}");
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.windows(codec.len()).any(|w| w == codec), "{format} holds {}", String::from_utf8_lossy(codec));
        assert_eq!(decode(&out, [320, 180]).len(), 39, "{format}");
    }
}

#[test]
fn a_sound_track_plays_under_the_frames_until_they_end() {
    if !ffmpeg() {
        return;
    }
    // Two seconds of tone under `close`'s three-second hold.
    let tone = scratch("tone.wav");
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
        .arg(&tone)
        .status()
        .unwrap();
    assert!(made.success());
    let out = scratch("sound.mp4");
    let req = Request {
        format: "mp4".into(),
        states: Some(vec!["close".into()]),
        out: Some(out.clone()),
        size: Some("320x180".into()),
        fps: Some(10),
        audio: Some(tone),
    };
    let video = export(&scaena_ops::open(Path::new(REVENUE)).unwrap(), &req).unwrap();
    assert_eq!(video.frames, Some(30));
    let probe = |stream: &str| {
        let out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                stream,
                "-show_entries",
                "stream=codec_name,duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&out)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let (picture, sound) = (probe("v:0"), probe("a:0"));
    assert!(picture.starts_with("h264,3.0"), "{picture}");
    // The tone, carried on in silence to the end of the frames.
    let (codec, seconds) = sound.split_once(',').unwrap();
    let seconds: f64 = seconds.parse().unwrap();
    assert_eq!(codec, "aac");
    assert!((2.9..=3.1).contains(&seconds), "{sound}");
}

#[test]
fn a_video_says_what_it_needs() {
    let bundle = scaena_ops::open(Path::new(REVENUE)).unwrap();
    let ask = |req: Request| export(&bundle, &req).unwrap_err().to_string();
    let mp4 = |out: Option<PathBuf>| Request { format: "mp4".into(), out, ..Request::default() };
    assert!(ask(mp4(None)).contains("--out FILE"));
    let odd = Request { size: Some("321x180".into()), ..mp4(Some(scratch("odd.mp4"))) };
    assert!(ask(odd).contains("aspect ratio"));
    // 318 × 179 has the canvas's shape, and 4:2:0 color cannot halve 179 rows.
    let odd = Request { size: Some("318x179".into()), ..mp4(Some(scratch("odd.mp4"))) };
    assert!(ask(odd).contains("even"));
    let fast = Request { fps: Some(1000), ..mp4(Some(scratch("fast.mp4"))) };
    assert!(ask(fast).contains("1 to 240"));
    let silent = Request { audio: Some(scratch("no-such.wav")), ..mp4(Some(scratch("silent.mp4"))) };
    assert!(ask(silent).contains("no such file"));
    let png = Request { format: "png".into(), audio: Some(scratch("tone.wav")), ..mp4(Some(scratch("pngs"))) };
    assert!(ask(png).contains("--audio"));
    // A deck with no hold and no motion has nothing to play.
    let still = scaena_ops::open(Path::new("../../tests/fixtures/torture.scaena")).unwrap();
    let req = Request { states: Some(vec!["liga".into()]), ..mp4(Some(scratch("still.mp4"))) };
    assert!(export(&still, &req).unwrap_err().to_string().contains("0 ms long"));
}
