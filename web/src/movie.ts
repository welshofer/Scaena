// The deck as a movie, made in the editor's worker (lift-up 20261031 FUNC-2): the timeline
// `scaena export --format webm` plays, each frame painted by the engine's CPU painter, encoded
// by the browser's own `VideoEncoder` (WebCodecs), VP9 where it has it, else VP8, and muxed as
// WebM here (`webm.ts`). A frame drawn as the one before it is neither painted nor encoded again:
// the one before it shows on. Nothing is sent anywhere.
import { type Chunk, webm } from "./webm";

/** What the engine gives a movie: its plan, and each frame. */
export interface Frames {
  movie(fps: number): string;
  movieFrame(fps: number, k: number, width: number, before?: string): { digest: string; painted: boolean; size?: Uint32Array | number[]; pixels(): Uint8ClampedArray };
}

/** Whether this browser can encode a movie: where it cannot, the dialog says so. */
export const encodes = () => typeof VideoEncoder !== "undefined";

/** Why a movie cannot be made here, in a person's words. */
export const UNENCODED = "This browser cannot encode a movie. Export it in Chrome, Edge, Firefox, or Safari, or with `scaena export --format webm`.";

/** The codecs tried, best first: WebCodecs' name, and Matroska's. */
const CODECS: [string, string][] = [
  ["vp09.00.10.08", "V_VP9"],
  ["vp8", "V_VP8"],
];

/** The longest a key frame is from the one before it, µs: where a seek may land. */
const KEYS_US = 2_000_000;

/** The deck as a WebM movie of `fps` frames a second, `width` pixels wide, titled `title`;
 * `progress` hears how many frames are made of how many. */
export async function movie(frames: Frames, fps: number, width: number, title: string, progress: (done: number, of: number) => void): Promise<Uint8Array> {
  if (!encodes()) throw new Error(UNENCODED);
  const plan = JSON.parse(frames.movie(fps)) as { frames: number; fps: number; durationMs: number };
  const first = frames.movieFrame(fps, 0, width);
  const [w, h] = [...(first.size ?? [0, 0])];
  // A frame of 4:2:0 colour is an even number of pixels each way: an odd last row or column goes.
  const [even, high] = [w & ~1, h & ~1];
  if (even < 2 || high < 2) throw new Error(`a movie ${w} × ${h} pixels is too small to encode`);
  const config = (codec: string): VideoEncoderConfig => ({
    codec,
    width: even,
    height: high,
    framerate: fps,
    // About a tenth of a bit a pixel a frame: slides hold still, and text stays sharp.
    bitrate: Math.max(500_000, Math.round(even * high * fps * 0.1)),
    latencyMode: "quality",
  });
  let chosen: [string, string] | undefined;
  for (const codec of CODECS) {
    if ((await VideoEncoder.isConfigSupported(config(codec[0])).catch(() => undefined))?.supported) {
      chosen = codec;
      break;
    }
  }
  if (!chosen) throw new Error(`This browser encodes neither VP9 nor VP8 at ${even} × ${high} pixels: try a smaller width.`);
  const chunks: Chunk[] = [];
  let failed: unknown;
  const encoder = new VideoEncoder({
    output: (chunk) => {
      const data = new Uint8Array(chunk.byteLength);
      chunk.copyTo(data);
      chunks.push({ data, timestamp: chunk.timestamp, key: chunk.type === "key" });
    },
    error: (e) => (failed = e),
  });
  encoder.configure(config(chosen[0]));
  try {
    let digest: string | undefined;
    let key = -Infinity;
    for (let k = 0; k < plan.frames; k++) {
      if (failed) throw failed;
      const frame = k === 0 ? first : frames.movieFrame(fps, k, width, digest);
      if (frame.painted) {
        digest = frame.digest;
        const timestamp = Math.round((k * 1_000_000) / fps);
        const video = new VideoFrame(frame.pixels(), {
          format: "RGBA",
          codedWidth: w,
          codedHeight: h,
          visibleRect: { x: 0, y: 0, width: even, height: high },
          timestamp,
        });
        const keyFrame = timestamp - key >= KEYS_US;
        if (keyFrame) key = timestamp;
        encoder.encode(video, { keyFrame });
        video.close();
        // The encoder kept a few frames behind, so the frames waiting stay few.
        while (encoder.encodeQueueSize > 4) await new Promise((ok) => encoder.addEventListener("dequeue", ok, { once: true }));
      }
      if (k % fps === 0) progress(k, plan.frames);
    }
    await encoder.flush();
    if (failed) throw failed;
  } finally {
    if (encoder.state !== "closed") encoder.close();
  }
  progress(plan.frames, plan.frames);
  chunks.sort((a, b) => a.timestamp - b.timestamp);
  return webm({ codec: chosen[1], width: even, height: high, durationMs: plan.durationMs, title }, chunks);
}
