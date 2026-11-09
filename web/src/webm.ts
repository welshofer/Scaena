// A WebM file of one video track (lift-up 20261031 FUNC-2): the frames a `VideoEncoder` made,
// muxed in memory as Matroska writes them, with what a player needs to play and seek it: the
// EBML header, the segment's info (its length), the track, a cluster at each key frame, and the
// cues that find them. No dependency: the few elements a video needs, written by hand.

/** One frame the encoder made: its bytes, when it shows, µs, and whether it is a key frame. */
export interface Chunk {
  data: Uint8Array;
  timestamp: number;
  key: boolean;
}

/** What the file says of its track: its codec as Matroska names it (`V_VP9`, `V_VP8`), its size
 * in pixels, how long it plays, ms, and its title, where it has one. */
export interface Track {
  codec: string;
  width: number;
  height: number;
  durationMs: number;
  title?: string;
}

/** `n` as an EBML variable-length size: the fewest bytes that hold it. */
function size(n: number): Uint8Array {
  for (let length = 1; length <= 8; length++) {
    // All ones is reserved for a size not known.
    if (n < 2 ** (7 * length) - 1) {
      const out = new Uint8Array(length);
      let v = n;
      for (let i = length - 1; i >= 0; i--) {
        out[i] = v % 256;
        v = Math.floor(v / 256);
      }
      out[0] |= 1 << (8 - length);
      return out;
    }
  }
  throw new Error(`${n} bytes are more than a WebM element holds`);
}

/** An element ID as its bytes (the ID's own marker bits included, as Matroska's tables give it). */
function id(n: number): Uint8Array {
  const bytes: number[] = [];
  for (let v = n; v > 0; v = Math.floor(v / 256)) bytes.unshift(v % 256);
  return Uint8Array.from(bytes);
}

function join(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/** The element `tag` holding `body`. */
function element(tag: number, ...body: Uint8Array[]): Uint8Array {
  const inside = join(body);
  return join([id(tag), size(inside.length), inside]);
}

/** An unsigned integer element, in the fewest bytes. */
function uint(tag: number, n: number): Uint8Array {
  const bytes: number[] = [];
  for (let v = n; v > 0 || bytes.length === 0; v = Math.floor(v / 256)) bytes.unshift(v % 256);
  return element(tag, Uint8Array.from(bytes));
}

function float(tag: number, n: number): Uint8Array {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setFloat64(0, n);
  return element(tag, bytes);
}

function text(tag: number, s: string): Uint8Array {
  return element(tag, new TextEncoder().encode(s));
}

const EBML = 0x1a45dfa3;
const SEGMENT = 0x18538067;
const INFO = 0x1549a966;
const TRACKS = 0x1654ae6b;
const CLUSTER = 0x1f43b675;
const CUES = 0x1c53bb6b;

/** The farthest a block stands from its cluster's start, ms: its offset is a signed 16 bits. */
const REACH = 32_000;

/** `chunks`, in the order they show, as a WebM file of `track`. */
export function webm(track: Track, chunks: Chunk[]): Uint8Array {
  const header = element(
    EBML,
    uint(0x4286, 1), // EBMLVersion
    uint(0x42f7, 1), // EBMLReadVersion
    uint(0x42f2, 4), // EBMLMaxIDLength
    uint(0x42f3, 8), // EBMLMaxSizeLength
    text(0x4282, "webm"), // DocType
    uint(0x4287, 4), // DocTypeVersion
    uint(0x4285, 2), // DocTypeReadVersion
  );
  const info = element(
    INFO,
    uint(0x2ad7b1, 1_000_000), // TimestampScale: a millisecond
    float(0x4489, Math.max(track.durationMs, 1)), // Duration
    text(0x4d80, "Scaena"), // MuxingApp
    text(0x5741, "Scaena"), // WritingApp
    ...(track.title ? [text(0x7ba9, track.title)] : []),
  );
  const tracks = element(
    TRACKS,
    element(
      0xae, // TrackEntry
      uint(0xd7, 1), // TrackNumber
      uint(0x73c5, 1), // TrackUID
      uint(0x83, 1), // TrackType: video
      uint(0x9c, 0), // FlagLacing
      text(0x86, track.codec), // CodecID
      element(0xe0, uint(0xb0, track.width), uint(0xba, track.height)), // Video
    ),
  );
  // A cluster at each key frame, and wherever a block would stand too far from its cluster.
  const clusters: { start: number; key: boolean; blocks: Uint8Array[] }[] = [];
  for (const chunk of chunks) {
    const ms = Math.round(chunk.timestamp / 1000);
    const last = clusters.at(-1);
    const cluster = !last || chunk.key || ms - last.start > REACH ? { start: ms, key: chunk.key, blocks: [] } : last;
    if (cluster !== last) clusters.push(cluster);
    const head = new Uint8Array(4);
    head[0] = 0x81; // track 1, as a one-byte vint
    new DataView(head.buffer).setInt16(1, ms - cluster.start);
    head[3] = chunk.key ? 0x80 : 0;
    cluster.blocks.push(element(0xa3, head, chunk.data)); // SimpleBlock
  }
  const written = clusters.map((c) => element(CLUSTER, uint(0xe7, c.start), ...c.blocks));
  // Each cluster that opens on a key frame, found by where it stands from the segment's first
  // byte inside it: where a seek may start decoding.
  let at = info.length + tracks.length;
  const points: Uint8Array[] = [];
  for (const [i, c] of clusters.entries()) {
    if (c.key) points.push(element(0xbb, uint(0xb3, c.start), element(0xb7, uint(0xf7, 1), uint(0xf1, at))));
    at += written[i].length;
  }
  const cues = points.length ? [element(CUES, ...points)] : [];
  return join([header, element(SEGMENT, info, tracks, ...written, ...cues)]);
}
