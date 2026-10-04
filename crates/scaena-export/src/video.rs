//! Video (PLAN 1.21; SPEC §10): frames painted by the CPU painter and piped to `ffmpeg`,
//! which encodes them.
//!
//! - The caller hands over each frame's display list in order: the global timeline
//!   sampled at the frame rate (SPEC §2.4). Frames are deterministic: each is a pure
//!   function of the document and its time, painted by the CPU painter (SPEC §13.6).
//! - A frame that draws what the one before it drew (a hold with nothing moving) is not
//!   painted again. The rest are painted on every core, a batch at a time, and written
//!   in order.
//! - ffmpeg takes them as raw RGB, composited over black, converts them to BT.709 YUV
//!   (video range), tags them so, and encodes them: H.264 in MP4, VP9 in WebM, or
//!   ProRes 422 HQ in QuickTime.
//! - A sound track, if one is given, is laid under the frames from the first: cut where
//!   they end, or carried on in silence until they do. AAC in MP4, Opus in WebM, 16-bit
//!   PCM in QuickTime.
//! - Chapters, if they are given, mark where each part starts and ends, by title: the
//!   caller's beats (SPEC §10).
//! - The video is written beside `out`, under a name of its own, and renamed to it once
//!   ffmpeg has finished. A failed export leaves no half-written file, and an earlier one
//!   stays as it was. Two exports of one file never write over each other's: the last to
//!   finish is the one that stays.

use crate::ExportError;
use scaena_core::displaylist::DisplayList;
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, Painter as _};
use schemars::JsonSchema;
use serde::Serialize;
use std::ffi::OsString;
use std::io::{Read as _, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A video's codec, in its container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// H.264 in MP4: `mp4`.
    H264,
    /// VP9 in WebM: `webm`.
    Vp9,
    /// ProRes 422 HQ in QuickTime: `prores`.
    ProRes,
}

impl Codec {
    /// What a format names it in errors: its export format.
    pub fn name(self) -> &'static str {
        match self {
            Codec::H264 => "mp4",
            Codec::Vp9 => "webm",
            Codec::ProRes => "prores",
        }
    }
}

/// How a video is encoded.
#[derive(Debug, Clone)]
pub struct VideoSettings {
    pub codec: Codec,
    /// Pixels to the canvas unit.
    pub scale: f32,
    /// Frames a second.
    pub fps: u32,
    /// A sound track: any file ffmpeg reads.
    pub audio: Option<PathBuf>,
    /// Its chapters, in order.
    pub chapters: Vec<Chapter>,
    /// The ffmpeg to run.
    pub ffmpeg: PathBuf,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            scale: 1.0,
            fps: 60,
            audio: None,
            chapters: Vec::new(),
            ffmpeg: PathBuf::from("ffmpeg"),
        }
    }
}

/// A part of a video a player can skip to: a beat of the spine, by its claim.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Chapter {
    /// The beat it plays, if a beat names its states.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beat: Option<String>,
    /// The beat's claim, or the id of a slide no beat names.
    pub title: String,
    /// Where it starts, ms into the video.
    pub start: f64,
    /// Where it ends, ms: where the next starts, or the video ends.
    pub end: f64,
}

/// What a video holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encoded {
    pub frames: u64,
    /// The frames painted: the rest drew what the frame before them drew.
    pub painted: u64,
    /// Pixels: width, height.
    pub size: [u32; 2],
}

/// The most frames a second a video takes.
pub const MAX_FPS: u32 = 240;

/// Encodes the frames `next` hands over, in order, into `out`, each drawing a canvas of
/// `canvas` units. `next` returns `None` after the last.
pub fn encode(
    out: &Path,
    canvas: [f32; 2],
    settings: &VideoSettings,
    assets: &Assets,
    mut next: impl FnMut() -> Option<Result<DisplayList, ExportError>>,
) -> Result<Encoded, ExportError> {
    let bad = |m: String| ExportError::Video(m);
    if !(1..=MAX_FPS).contains(&settings.fps) {
        return Err(bad(format!("{} frames a second: a video takes 1 to {MAX_FPS}", settings.fps)));
    }
    let size = canvas.map(|c| (c * settings.scale).round());
    if !size.iter().all(|s| (2.0..=f32::from(u16::MAX)).contains(s)) {
        return Err(bad(format!("a {} × {} px video", size[0], size[1])));
    }
    let size = size.map(|s| s as u32);
    if size.iter().any(|s| s % 2 == 1) {
        // 4:2:0 halves both sides for color; 4:2:2 halves the width.
        return Err(bad(format!(
            "{} needs a width and height that are even, not {} × {}: give --size the canvas's aspect ratio in even pixels",
            settings.codec.name(),
            size[0],
            size[1]
        )));
    }
    if let Some(audio) = settings.audio.as_deref().filter(|a| !a.is_file()) {
        return Err(bad(format!("--audio {}: no such file", audio.display())));
    }
    let (partial, chapters) = beside_once(out);
    if !settings.chapters.is_empty() {
        std::fs::write(&chapters, metadata(&settings.chapters))
            .map_err(|e| bad(format!("writing {}: {e}", chapters.display())))?;
    }
    let listed = (!settings.chapters.is_empty()).then_some(chapters.as_path());
    let spawned = Command::new(&settings.ffmpeg)
        .args(args(settings, size, &partial, listed))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => bad(format!(
                "`{}` is not on the PATH: video export pipes its frames to ffmpeg (https://ffmpeg.org)",
                settings.ffmpeg.display()
            )),
            _ => bad(format!("starting {}: {e}", settings.ffmpeg.display())),
        });
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_file(&chapters);
            return Err(e);
        }
    };
    let mut stderr = child.stderr.take().expect("stderr is piped");
    // Read as it comes, so ffmpeg never waits on a full pipe while we wait on it.
    let said = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    // The frames, then the end of ffmpeg's input: its stdin closes as the block ends.
    let pumped = {
        let mut stdin = child.stdin.take().expect("stdin is piped");
        pump(&mut stdin, size, settings.scale, assets, &mut next)
    };
    if pumped.is_err() {
        // The frames stopped short: what ffmpeg has is not the video.
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| bad(format!("waiting for ffmpeg: {e}")));
    let _ = std::fs::remove_file(&chapters);
    let status = status?;
    let said = said.join().unwrap_or_default();
    let said = said.trim();
    let finished = match pumped {
        Ok(encoded) if status.success() => {
            std::fs::rename(&partial, out).map_err(|e| bad(format!("writing {}: {e}", out.display())))?;
            return Ok(encoded);
        }
        // ffmpeg stopped reading: why is what it said.
        Err(Pipe) | Ok(_) => bad(format!("ffmpeg failed ({status}){}", if said.is_empty() { "" } else { ": " }) + said),
        Err(Stopped(e)) => e,
    };
    let _ = std::fs::remove_file(&partial);
    Err(finished)
}

/// Why frames stopped going to ffmpeg.
enum Halt {
    /// It closed its input.
    Pipe,
    /// A frame could not be made or painted.
    Stopped(ExportError),
}
use Halt::{Pipe, Stopped};

/// Paints the frames `next` hands over and writes them to ffmpeg, in order.
fn pump(
    stdin: &mut impl Write,
    size: [u32; 2],
    scale: f32,
    assets: &Assets,
    next: &mut impl FnMut() -> Option<Result<DisplayList, ExportError>>,
) -> Result<Encoded, Halt> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(16);
    let batch = threads * 2;
    let (mut frames, mut painted) = (0_u64, 0_u64);
    // The last frame written: what it drew, and its pixels.
    let mut last: Option<(DisplayList, Arc<Vec<u8>>)> = None;
    loop {
        let mut lists = Vec::with_capacity(batch);
        while lists.len() < batch {
            match next() {
                Some(list) => lists.push(list.map_err(Stopped)?),
                None => break,
            }
        }
        if lists.is_empty() {
            return Ok(Encoded { frames, painted, size });
        }
        // A frame is painted unless it draws what the one before it drew.
        let todo: Vec<usize> = (0..lists.len())
            .filter(|&i| match i {
                0 => last.as_ref().is_none_or(|(list, _)| *list != lists[0]),
                i => lists[i] != lists[i - 1],
            })
            .collect();
        let mut pixels = paint(&lists, &todo, threads, scale, assets);
        let mut current = last.take().map(|(_, px)| px);
        for slot in &mut pixels {
            if let Some(px) = slot.take() {
                let px = px.map_err(Stopped)?;
                if px.len() != (size[0] * size[1] * 3) as usize {
                    return Err(Stopped(ExportError::Video("a frame painted at another size".into())));
                }
                current = Some(Arc::new(px));
                painted += 1;
            }
            let px = current.as_ref().expect("the first frame is painted");
            stdin.write_all(px).map_err(|_| Pipe)?;
            frames += 1;
        }
        let list = lists.pop().expect("a batch holds a frame");
        last = current.map(|px| (list, px));
    }
}

/// The frames `todo` names, painted on up to `threads` cores: for each frame of `lists`,
/// its RGB pixels, or none where it was not asked for.
fn paint(
    lists: &[DisplayList],
    todo: &[usize],
    threads: usize,
    scale: f32,
    assets: &Assets,
) -> Vec<Option<Result<Vec<u8>, ExportError>>> {
    let mut out: Vec<Option<Result<Vec<u8>, ExportError>>> = (0..lists.len()).map(|_| None).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads.min(todo.len()))
            .map(|_| {
                s.spawn(|| {
                    // Frames are already painted on every core, so a frame's shaders take one.
                    let mut painter = CpuPainter { threads: 1, ..CpuPainter::default() };
                    let mut done = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&i) = todo.get(k) else { return done };
                        let px = painter
                            .paint(&lists[i], assets, scale)
                            .map(|r| rgb(&r.rgba))
                            .map_err(|e| ExportError::Video(e.to_string()));
                        done.push((i, px));
                    }
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().expect("a painter thread does not panic")).collect::<Vec<_>>()
    });
    for (i, px) in done {
        out[i] = Some(px);
    }
    out
}

/// Straight-alpha RGBA as RGB over black.
fn rgb(rgba: &[u8]) -> Vec<u8> {
    let over = |c: u8, a: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
    let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
    for &[r, g, b, a] in rgba.as_chunks::<4>().0 {
        if a == 255 {
            out.extend_from_slice(&[r, g, b]);
        } else {
            out.extend_from_slice(&[over(r, a), over(g, a), over(b, a)]);
        }
    }
    out
}

/// Where this export writes until the video is whole: the video, and its chapters for
/// ffmpeg. Beside `out`, so renaming the video is a move, and named for this export alone.
fn beside_once(out: &Path) -> (PathBuf, PathBuf) {
    static EXPORTS: AtomicU64 = AtomicU64::new(0);
    let this = format!("{}-{}", std::process::id(), EXPORTS.fetch_add(1, Ordering::Relaxed));
    (beside(out, &format!(".{this}.partial")), beside(out, &format!(".{this}.chapters")))
}

/// A file beside `out`, named for it with `suffix`.
fn beside(out: &Path, suffix: &str) -> PathBuf {
    let mut name = out.file_name().map_or_else(OsString::new, |n| n.to_os_string());
    name.push(suffix);
    out.with_file_name(name)
}

/// `chapters` as ffmpeg's metadata file: each from its start to its end, ms, by title.
fn metadata(chapters: &[Chapter]) -> String {
    // `=`, `;`, `#`, `\`, and line breaks are the format's own, so a title escapes them.
    let escape = |s: &str| {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if matches!(c, '=' | ';' | '#' | '\\' | '\n') {
                out.push('\\');
            }
            out.push(c);
        }
        out
    };
    let mut out = String::from(";FFMETADATA1\n");
    for c in chapters {
        let (start, end) = (c.start.round().max(0.0) as u64, c.end.round().max(0.0) as u64);
        out += &format!(
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART={start}\nEND={}\ntitle={}\n",
            end.max(start + 1),
            escape(&c.title)
        );
    }
    out
}

/// ffmpeg's arguments: raw RGB frames on stdin, then the sound track and the chapters, if
/// there are any; the codecs' settings; `out`.
fn args(settings: &VideoSettings, [w, h]: [u32; 2], out: &Path, chapters: Option<&Path>) -> Vec<OsString> {
    let (pixels, codec, sound): (&str, &[&str], &[&str]) = match settings.codec {
        Codec::H264 => (
            "yuv420p",
            &["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-movflags", "+faststart", "-f", "mp4"],
            &["-c:a", "aac", "-b:a", "192k"],
        ),
        Codec::Vp9 => (
            "yuv420p",
            &[
                "-c:v",
                "libvpx-vp9",
                "-crf",
                "30",
                "-b:v",
                "0",
                "-deadline",
                "good",
                "-cpu-used",
                "2",
                "-row-mt",
                "1",
                "-f",
                "webm",
            ],
            &["-c:a", "libopus", "-b:a", "160k"],
        ),
        Codec::ProRes => (
            "yuv422p10le",
            &["-c:v", "prores_ks", "-profile:v", "3", "-vendor", "apl0", "-f", "mov"],
            &["-c:a", "pcm_s16le"],
        ),
    };
    let size = format!("{w}x{h}");
    let fps = settings.fps.to_string();
    let filter = format!("scale=out_color_matrix=bt709:out_range=tv,format={pixels}");
    let mut args: Vec<OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-video_size",
        &size,
        "-framerate",
        &fps,
        "-i",
        "pipe:0",
        "-vf",
        &filter,
        "-pix_fmt",
        pixels,
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-colorspace",
        "bt709",
        "-color_range",
        "tv",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.extend(codec.iter().map(OsString::from));
    // Inputs go before what they feed: the others after the frames' input, numbered on.
    let (mut inputs, mut maps, mut next): (Vec<OsString>, Vec<OsString>, usize) = (Vec::new(), Vec::new(), 1);
    if settings.audio.is_some() || chapters.is_some() {
        maps.extend(["-map", "0:v:0"].map(OsString::from));
    }
    if let Some(audio) = &settings.audio {
        inputs.extend(["-i".into(), audio.as_os_str().to_os_string()]);
        // Silence after the track runs out, and the frames decide where the video ends.
        maps.extend(["-map".into(), format!("{next}:a:0").into()]);
        maps.extend(["-af", "apad", "-shortest"].iter().chain(sound).map(OsString::from));
        next += 1;
    }
    // The video's chapters are ours or none: never a sound file's.
    if let Some(chapters) = chapters {
        inputs.extend(["-f".into(), "ffmetadata".into(), "-i".into(), chapters.as_os_str().to_os_string()]);
        maps.extend(["-map_chapters".into(), next.to_string().into()]);
    } else {
        maps.extend(["-map_chapters", "-1"].map(OsString::from));
    }
    let at = args.iter().position(|a| a == "pipe:0").expect("the frames' input") + 1;
    args.splice(at..at, inputs);
    args.extend(maps);
    // No metadata from the inputs. A bare `-map_metadata -1` would strip the chapters'
    // titles too, and in MP4 and QuickTime their track.
    args.extend(["-map_metadata:g", "-1", "-map_metadata:s", "-1", "-fflags", "+bitexact", "-y"].map(OsString::from));
    args.push(out.as_os_str().to_os_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_lays_translucent_pixels_over_black() {
        assert_eq!(rgb(&[10, 20, 30, 255, 200, 100, 50, 128, 255, 255, 255, 0]), [10, 20, 30, 100, 50, 25, 0, 0, 0]);
    }

    #[test]
    fn the_video_is_written_beside_where_it_goes() {
        let out = Path::new("out/deck.mp4");
        let ((video, chapters), (again, _)) = (beside_once(out), beside_once(out));
        for written in [&video, &chapters, &again] {
            assert_eq!(written.parent(), out.parent());
            assert!(written.file_name().unwrap().to_str().unwrap().starts_with("deck.mp4."), "{}", written.display());
        }
        assert!(video.to_str().unwrap().ends_with(".partial") && chapters.to_str().unwrap().ends_with(".chapters"));
        assert_ne!(video, again, "each export writes a file of its own");
        let args = args(
            &VideoSettings { codec: Codec::ProRes, ..VideoSettings::default() },
            [1920, 1080],
            Path::new("x"),
            None,
        );
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert!(args.windows(2).any(|w| w == ["-video_size", "1920x1080"]));
        assert!(args.windows(2).any(|w| w == ["-c:v", "prores_ks"]));
        assert!(args.windows(2).any(|w| w == ["-pix_fmt", "yuv422p10le"]));
        assert_eq!(args.last(), Some(&"x"));
        assert!(args.windows(2).any(|w| w == ["-map_chapters", "-1"]), "no chapters, not even a sound file's");
        let sound = VideoSettings { audio: Some("voice.wav".into()), ..VideoSettings::default() };
        let args = super::args(&sound, [1920, 1080], Path::new("x"), Some(Path::new("x.chapters")));
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        let inputs: Vec<&str> = args.windows(2).filter(|w| w[0] == "-i").map(|w| w[1]).collect();
        assert_eq!(inputs, ["pipe:0", "voice.wav", "x.chapters"]);
        assert!(args.windows(2).any(|w| w == ["-c:a", "aac"]) && args.contains(&"-shortest"));
        assert!(
            args.windows(2).any(|w| w == ["-map", "1:a:0"]) && args.windows(2).any(|w| w == ["-map_chapters", "2"])
        );
    }

    /// Held to run stand-ins, and alone to write one: a child forked while a script is
    /// being written holds it open for writing, and running the script then fails
    /// (ETXTBSY), so no test spawns while another writes.
    static SPAWNS: std::sync::RwLock<()> = std::sync::RwLock::new(());

    fn may_spawn() -> std::sync::RwLockReadGuard<'static, ()> {
        SPAWNS.read().unwrap_or_else(|e| e.into_inner())
    }

    /// A stand-in for ffmpeg, at `dir/name`: a shell script that runs `body` with `$out`,
    /// the file it is given last.
    #[cfg(unix)]
    fn stand_in(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let _alone = SPAWNS.write().unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\nfor out; do :; done\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn filled(color: [u8; 4]) -> DisplayList {
        use scaena_core::displaylist::{Color, FillRule, Op, Paint, Path as DlPath};
        let mut list = DisplayList::new([4.0, 2.0]);
        list.ops.push(Op::Fill {
            path: DlPath::rect([0.0, 0.0, 4.0, 2.0]),
            rule: FillRule::NonZero,
            paint: Paint::Solid(Color(color)),
        });
        list
    }

    #[test]
    #[cfg(unix)]
    fn frames_reach_ffmpeg_in_order_and_a_frame_like_the_last_is_not_painted_again() {
        let dir = std::env::temp_dir().join(format!("scaena-video-{}", std::process::id()));
        let ffmpeg = stand_in(&dir, "copies", "cat > \"$out\"");
        let out = dir.join("deck.mp4");
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 128]);
        let mut lists = vec![filled(red), filled(red), filled(blue), filled(red)].into_iter();
        let settings = VideoSettings { ffmpeg, fps: 10, ..VideoSettings::default() };
        let spawning = may_spawn();
        let encoded = encode(&out, [4.0, 2.0], &settings, &Assets::new(), || lists.next().map(Ok)).unwrap();
        drop(spawning);
        assert_eq!(encoded, Encoded { frames: 4, painted: 3, size: [4, 2] });
        // Each frame's pixels, in order: half-transparent blue over black.
        let px = |c: [u8; 3]| c.repeat(8);
        let want = [px([255, 0, 0]), px([255, 0, 0]), px([0, 0, 128]), px([255, 0, 0])].concat();
        assert_eq!(std::fs::read(&out).unwrap(), want);
        assert_eq!(leftovers(&dir), Vec::<String>::new(), "the partial file is renamed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What an export left beside its video: a partial file or a chapters file.
    fn leftovers(dir: &Path) -> Vec<String> {
        let names = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap());
        names.filter(|n| n.ends_with(".partial") || n.ends_with(".chapters")).collect()
    }

    #[test]
    #[cfg(unix)]
    fn two_exports_of_one_file_never_write_over_each_other() {
        let dir = std::env::temp_dir().join(format!("scaena-video-twice-{}", std::process::id()));
        // ffmpeg that writes slowly, so the two exports overlap.
        let ffmpeg = stand_in(&dir, "slow", "cat > \"$out\"; sleep 0.3");
        let out = dir.join("deck.mp4");
        let export = |color: [u8; 4], frames: usize| {
            let (out, ffmpeg) = (out.clone(), ffmpeg.clone());
            std::thread::spawn(move || {
                let settings = VideoSettings { ffmpeg, fps: 10, chapters: chaptered(), ..VideoSettings::default() };
                let mut lists = std::iter::repeat_with(|| filled(color)).take(frames);
                encode(&out, [4.0, 2.0], &settings, &Assets::new(), || lists.next().map(Ok))
            })
        };
        let spawning = may_spawn();
        let (first, second) = (export([255, 0, 0, 255], 3), export([0, 0, 255, 255], 5));
        let (first, second) = (first.join().unwrap().unwrap(), second.join().unwrap().unwrap());
        drop(spawning);
        assert_eq!((first.frames, second.frames), (3, 5));
        // What stays is one export's video, whole: never the two mixed.
        let video = std::fs::read(&out).unwrap();
        let (red, blue) = ([255, 0, 0].repeat(8 * 3), [0, 0, 255].repeat(8 * 5));
        assert!(video == red || video == blue, "{} bytes, neither export's", video.len());
        assert_eq!(leftovers(&dir), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn chaptered() -> Vec<Chapter> {
        vec![Chapter { beat: Some("all".into()), title: "All".into(), start: 0.0, end: 100.0 }]
    }

    #[test]
    #[cfg(unix)]
    fn a_failed_encode_says_why_and_leaves_no_file() {
        let dir = std::env::temp_dir().join(format!("scaena-video-fails-{}", std::process::id()));
        let refuses = stand_in(&dir, "refuses", "echo \"Unknown encoder 'libx264'\" >&2; exit 1");
        let out = dir.join("deck.mp4");
        let settings = VideoSettings { ffmpeg: refuses, ..VideoSettings::default() };
        let mut lists = std::iter::repeat_with(|| filled([9, 9, 9, 255])).take(100);
        let spawning = may_spawn();
        let err = encode(&out, [4.0, 2.0], &settings, &Assets::new(), || lists.next().map(Ok)).unwrap_err();
        drop(spawning);
        assert!(err.to_string().contains("Unknown encoder 'libx264'"), "{err}");
        assert!(!out.exists() && leftovers(&dir).is_empty());
        // A frame that cannot be made stops the video, and what ffmpeg wrote is removed.
        let copies = stand_in(&dir, "copies", "cat > \"$out\"");
        let settings = VideoSettings { ffmpeg: copies, ..VideoSettings::default() };
        let mut k = 0;
        let next = || {
            k += 1;
            Some(if k < 5 { Ok(filled([9, 9, 9, 255])) } else { Err(ExportError::Video("no frame 5".into())) })
        };
        let spawning = may_spawn();
        let err = encode(&out, [4.0, 2.0], &settings, &Assets::new(), next).unwrap_err();
        drop(spawning);
        assert_eq!(err.to_string(), "video: no frame 5");
        assert!(!out.exists() && leftovers(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chapters_are_ffmpegs_metadata_with_their_titles_escaped() {
        let chapters = [
            Chapter {
                beat: Some("doubled".into()),
                title: "Revenue doubled; = growth #1".into(),
                start: 0.0,
                end: 4660.4,
            },
            Chapter { beat: None, title: "close".into(), start: 4660.4, end: 4660.4 },
        ];
        assert_eq!(
            metadata(&chapters),
            ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=4660\ntitle=Revenue doubled\\; \\= growth \\#1\n\
             [CHAPTER]\nTIMEBASE=1/1000\nSTART=4660\nEND=4661\ntitle=close\n"
        );
    }

    #[test]
    fn odd_sizes_and_frame_rates_out_of_range_are_refused() {
        let none = || None;
        let out = Path::new("never.mp4");
        let settings = VideoSettings { ffmpeg: "/nonexistent/ffmpeg".into(), ..VideoSettings::default() };
        let err = encode(out, [1921.0, 1080.0], &settings, &Assets::new(), none).unwrap_err().to_string();
        assert!(err.contains("even"), "{err}");
        let fast = VideoSettings { fps: 1000, ..settings.clone() };
        let err = encode(out, [1920.0, 1080.0], &fast, &Assets::new(), none).unwrap_err().to_string();
        assert!(err.contains("1 to 240"), "{err}");
        let err = encode(out, [1920.0, 1080.0], &settings, &Assets::new(), none).unwrap_err().to_string();
        assert!(err.contains("is not on the PATH"), "{err}");
    }
}
