//! JPEG, read the same on every target (SPEC §3.3, §13; ADR-0017).
//!
//! A photo's pixels are part of a frame, and a frame is the same bytes natively and in the
//! browser. This decoder does integer arithmetic only, from one source on every target, with no
//! path chosen by the processor it runs on. Its arithmetic is zune-jpeg's portable path, so it
//! gives the bytes that path gives (`scaena-export`'s `photos_decode_as_zune_jpeg_does` holds the
//! two together on `tests/fixtures/jpeg`):
//! - the inverse DCT is stb_image's, in 32-bit integers that wrap;
//! - YCbCr becomes RGB by BT.601 at full range, in 14-bit fixed point;
//! - chroma sampled at half is brought back by the triangle filter, ¾ of the nearer sample and ¼
//!   of the farther, rounded after each direction, over the component's whole blocks; any other
//!   whole ratio repeats the nearest sample.
//!
//! It reads baseline, extended, and progressive JPEGs coded by Huffman tables at 8 bits a sample:
//! gray, or three components that are YCbCr (or RGB, where an Adobe marker or the components' ids
//! say so), sampled at factors that divide the largest, with restart intervals or without. It
//! refuses, saying why, arithmetic coding, lossless and hierarchical JPEGs, 12 bits a sample,
//! CMYK, and a height that only a DNL marker gives. A file cut short is drawn as far as it goes,
//! as libjpeg draws it; a code no Huffman table holds is an error.
//!
//! The EXIF orientation is read, and [`decode`] applies it: the picture as it is meant to be seen.
//! [`stripped`] is the file without what it says beyond its picture, for the exports that carry
//! the file itself.

use crate::displaylist::MAX_IMAGE_SIDE;

/// Why a JPEG cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct JpegError(String);

impl JpegError {
    fn damaged(why: &str) -> Self {
        JpegError(format!("a damaged JPEG: {why}"))
    }
}

type Result<T> = std::result::Result<T, JpegError>;

/// Whether `bytes` begin as a JPEG does.
pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// What a JPEG's header says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// As stored, before the orientation turns it.
    pub width: u32,
    pub height: u32,
    /// 1 (gray) or 3.
    pub components: u8,
    pub progressive: bool,
    /// The EXIF orientation, 1–8: how the stored picture turns to be seen. 1 where the file
    /// says none.
    pub orientation: u8,
}

impl Header {
    /// The header of the JPEG `bytes`, read as far as its first scan. A JPEG this decoder cannot
    /// read is refused here, saying why.
    pub fn read(bytes: &[u8]) -> Result<Header> {
        let mut decoder = Decoder::new(bytes)?;
        decoder.run(true)?;
        let frame = decoder.frame.as_ref().ok_or_else(|| JpegError::damaged("no frame before its first scan"))?;
        Ok(Header {
            width: frame.width as u32,
            height: frame.height as u32,
            components: frame.components.len() as u8,
            progressive: frame.progressive,
            orientation: decoder.orientation,
        })
    }

    /// The size as seen: width and height trade places where the orientation turns it a
    /// quarter.
    pub fn size(&self) -> (u32, u32) {
        if self.orientation >= 5 { (self.height, self.width) } else { (self.width, self.height) }
    }
}

/// A decoded picture: straight sRGB RGBA8, row-major, opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The JPEG `bytes` as they are meant to be seen: decoded, and turned by their EXIF orientation.
pub fn decode(bytes: &[u8]) -> Result<Decoded> {
    let mut decoder = Decoder::new(bytes)?;
    decoder.run(false)?;
    let stored = decoder.pixels()?;
    Ok(orient(stored, decoder.orientation))
}

/// The JPEG `bytes` decoded as they are stored, the orientation not applied: what other decoders
/// give, for a test to compare.
pub fn decode_stored(bytes: &[u8]) -> Result<Decoded> {
    let mut decoder = Decoder::new(bytes)?;
    decoder.run(false)?;
    decoder.pixels()
}

/// The JPEG `bytes` without what they say beyond the picture: where and when it was taken, the
/// camera, a thumbnail, XMP, a color profile, comments, and whatever follows its end (a second
/// picture, a gain map). What draws it stays as it is: the frame, its tables and scans, the JFIF
/// and Adobe markers, and the orientation, written as an EXIF that holds nothing else.
pub fn stripped(bytes: &[u8]) -> Result<Vec<u8>> {
    let orientation = Header::read(bytes)?.orientation;
    let mut decoder = Decoder::new(bytes)?;
    let mut out = vec![0xFF, 0xD8];
    let mut oriented = false;
    let mut scanned = false;
    loop {
        let start = decoder.pos;
        let marker = match decoder.marker() {
            Ok(m) => m,
            Err(_) if scanned => 0xD9,
            Err(e) => return Err(e),
        };
        match marker {
            0xD9 => {
                out.extend_from_slice(&[0xFF, 0xD9]);
                return Ok(out);
            }
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }
        // The orientation goes after the JFIF marker, which comes first, and before the rest.
        if !oriented && marker != 0xE0 {
            oriented = true;
            if orientation != 1 {
                out.extend_from_slice(&orientation_segment(orientation));
            }
        }
        decoder.segment()?;
        if !matches!(marker, 0xE1..=0xED | 0xEF | 0xFE) {
            out.extend_from_slice(&bytes[start..decoder.pos]);
        }
        if marker == 0xDA {
            // The scan's coded data, as it is, to the next marker.
            scanned = true;
            let from = decoder.pos;
            decoder.pos = coded_end(bytes, from);
            out.extend_from_slice(&bytes[from..decoder.pos]);
        }
    }
}

/// Where the coded data that starts at `from` ends: at the next marker that is not a restart.
fn coded_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() {
        if bytes[at] == 0xFF {
            match bytes.get(at + 1) {
                Some(0) | Some(0xD0..=0xD7) => {
                    at += 2;
                    continue;
                }
                Some(0xFF) => {
                    at += 1;
                    continue;
                }
                _ => return at,
            }
        }
        at += 1;
    }
    bytes.len()
}

/// An APP1 segment whose EXIF holds orientation `o` and nothing else.
fn orientation_segment(o: u8) -> Vec<u8> {
    let mut s = vec![0xFF, 0xE1, 0, 34];
    s.extend_from_slice(b"Exif\0\0MM\0\x2A\0\0\0\x08");
    // One entry: tag 0x0112, SHORT, one value; then no next IFD.
    s.extend_from_slice(&[0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, o, 0, 0, 0, 0, 0, 0]);
    s
}

/// The orientation an APP1 segment's EXIF gives, if it gives one from 1 to 8.
fn exif_orientation(payload: &[u8]) -> Option<u8> {
    let tiff = payload.strip_prefix(b"Exif\0\0")?;
    let little = match tiff.get(0..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let b = tiff.get(at..at.checked_add(2)?)?;
        Some(if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let b = tiff.get(at..at.checked_add(4)?)?;
        let b = [b[0], b[1], b[2], b[3]];
        Some(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let count = usize::from(u16_at(ifd)?);
    for i in 0..count {
        let entry = ifd.checked_add(2 + i * 12)?;
        if u16_at(entry)? == 0x0112 {
            // A SHORT, its value in the first two bytes of the entry's value field.
            if u16_at(entry + 2)? != 3 {
                return None;
            }
            let o = u16_at(entry + 8)?;
            return (1..=8).contains(&o).then_some(o as u8);
        }
    }
    None
}

/// `stored` turned by EXIF orientation `o`.
fn orient(stored: Decoded, o: u8) -> Decoded {
    if !(2..=8).contains(&o) {
        return stored;
    }
    let (w, h) = (stored.width as usize, stored.height as usize);
    let (ow, oh) = if o >= 5 { (h, w) } else { (w, h) };
    let mut rgba = vec![0; stored.rgba.len()];
    for y in 0..oh {
        for x in 0..ow {
            // The stored pixel seen at (x, y).
            let (sx, sy) = match o {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (y, h - 1 - x),
                7 => (w - 1 - y, h - 1 - x),
                _ => (w - 1 - y, x),
            };
            let from = (sy * w + sx) * 4;
            let to = (y * ow + x) * 4;
            rgba[to..to + 4].copy_from_slice(&stored.rgba[from..from + 4]);
        }
    }
    Decoded { width: ow as u32, height: oh as u32, rgba }
}

/// The natural (row-major) place of each coefficient, in the order a block codes them.
const ZIGZAG: [u8; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54,
    47, 55, 62, 63,
];

/// The most scans a progressive JPEG may have; an encoder writes about ten.
const MAX_SCANS: usize = 100;

/// A Huffman table, for decoding.
#[derive(Debug, Clone)]
struct Huffman {
    /// By the next 9 bits: the code's length and its symbol (`length << 8 | symbol`), or 0
    /// where the code is longer.
    fast: [u16; 512],
    /// By code length: the first code, how many codes, and the index of the first one's symbol.
    first: [i32; 17],
    count: [i32; 17],
    index: [usize; 17],
    symbols: Vec<u8>,
}

impl Huffman {
    fn new(counts: &[u8; 16], symbols: &[u8]) -> Result<Huffman> {
        let mut table =
            Huffman { fast: [0; 512], first: [0; 17], count: [0; 17], index: [0; 17], symbols: symbols.to_vec() };
        let mut code: i32 = 0;
        let mut k = 0;
        for length in 1..=16 {
            let n = i32::from(counts[length - 1]);
            table.first[length] = code;
            table.count[length] = n;
            table.index[length] = k;
            for _ in 0..n {
                if code >= 1 << length {
                    return Err(JpegError::damaged("a Huffman table with more codes than fit"));
                }
                if length <= 9 {
                    let shift = 9 - length;
                    let base = (code as usize) << shift;
                    let entry = ((length as u16) << 8) | u16::from(symbols[k]);
                    table.fast[base..base + (1 << shift)].fill(entry);
                }
                code += 1;
                k += 1;
            }
            code <<= 1;
        }
        Ok(table)
    }
}

/// The coded data of a scan, read a bit at a time. At a marker, or past the end, it reads zeros,
/// as libjpeg does.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    /// The bits read ahead, the next one highest.
    acc: u64,
    n: u32,
    /// At a marker or the end: what follows is zeros.
    stopped: bool,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Bits { data, pos, acc: 0, n: 0, stopped: false }
    }

    fn fill(&mut self) {
        while self.n <= 56 {
            let mut byte = 0;
            if !self.stopped {
                match self.data.get(self.pos) {
                    Some(0xFF) => {
                        if self.data.get(self.pos + 1) == Some(&0) {
                            byte = 0xFF;
                            self.pos += 2;
                        } else {
                            self.stopped = true;
                        }
                    }
                    Some(&b) => {
                        byte = b;
                        self.pos += 1;
                    }
                    None => self.stopped = true,
                }
            }
            self.acc |= u64::from(byte) << (56 - self.n);
            self.n += 8;
        }
    }

    /// The next `k` bits, 0 to 16.
    fn bits(&mut self, k: u32) -> u32 {
        if k == 0 {
            return 0;
        }
        if self.n < k {
            self.fill();
        }
        let v = (self.acc >> (64 - k)) as u32;
        self.acc <<= k;
        self.n -= k;
        v
    }

    fn bit(&mut self) -> bool {
        self.bits(1) == 1
    }

    /// A value of `s` bits, 0 to 16, sign-extended as JPEG codes it.
    fn extend(&mut self, s: u32) -> i32 {
        if s == 0 {
            return 0;
        }
        let v = self.bits(s) as i32;
        if v < 1 << (s - 1) { v - (1 << s) + 1 } else { v }
    }

    fn decode(&mut self, table: &Huffman) -> Result<u8> {
        if self.n < 16 {
            self.fill();
        }
        let peek = (self.acc >> 48) as i32;
        let fast = table.fast[(peek >> 7) as usize];
        if fast != 0 {
            let length = u32::from(fast >> 8);
            self.acc <<= length;
            self.n -= length;
            return Ok(fast as u8);
        }
        for length in 10..=16 {
            let code = peek >> (16 - length);
            let k = code - table.first[length];
            if k >= 0 && k < table.count[length] {
                self.acc <<= length;
                self.n -= length as u32;
                return Ok(table.symbols[table.index[length] + k as usize]);
            }
        }
        Err(JpegError::damaged("a code its Huffman table does not hold"))
    }

    /// Past a restart marker: what is left of the bits goes, and reading goes on after the
    /// marker. Where another marker comes first, the scan is cut short and reads zeros.
    fn restart(&mut self) {
        self.acc = 0;
        self.n = 0;
        self.stopped = false;
        while let Some(&b) = self.data.get(self.pos) {
            if b == 0xFF {
                match self.data.get(self.pos + 1) {
                    Some(0xD0..=0xD7) => {
                        self.pos += 2;
                        return;
                    }
                    Some(0) => self.pos += 2,
                    Some(0xFF) => self.pos += 1,
                    _ => break,
                }
            } else {
                self.pos += 1;
            }
        }
        self.stopped = true;
    }
}

/// A component of the frame.
#[derive(Debug, Clone)]
struct Component {
    id: u8,
    /// Sampling factors, 1–4.
    h: usize,
    v: usize,
    quant: usize,
    /// The quantization table, as it was when the component's first scan began.
    latched: Option<[u16; 64]>,
    /// Blocks across and down, in whole MCUs.
    bw: usize,
    bh: usize,
    /// Blocks across and down that the picture covers: what a scan of this component alone codes.
    cw: usize,
    ch: usize,
    /// Each block's coefficients, natural order: a progressive frame's, until its last scan.
    coefs: Vec<i16>,
    /// The samples, `bw × 8` wide and `bh × 8` high.
    plane: Vec<u8>,
    pred: i32,
}

#[derive(Debug, Clone)]
struct Frame {
    width: usize,
    height: usize,
    progressive: bool,
    components: Vec<Component>,
    hmax: usize,
    vmax: usize,
    mcux: usize,
    mcuy: usize,
}

/// How three components are colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Gray,
    YCbCr,
    Rgb,
}

struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
    quant: [Option<[u16; 64]>; 4],
    dc: [Option<Huffman>; 4],
    ac: [Option<Huffman>; 4],
    restart: usize,
    frame: Option<Frame>,
    jfif: bool,
    /// The Adobe marker's color transform.
    adobe: Option<u8>,
    orientation: u8,
    scans: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self> {
        if !bytes.starts_with(&[0xFF, 0xD8]) {
            return Err(JpegError("not a JPEG".into()));
        }
        Ok(Decoder {
            bytes,
            pos: 2,
            quant: [None; 4],
            dc: [None, None, None, None],
            ac: [None, None, None, None],
            restart: 0,
            frame: None,
            jfif: false,
            adobe: None,
            orientation: 1,
            scans: 0,
        })
    }

    /// The next marker, past any bytes before it: fill bytes, and what a scan left unread.
    fn marker(&mut self) -> Result<u8> {
        loop {
            let b = *self.bytes.get(self.pos).ok_or_else(|| JpegError::damaged("it ends before its picture does"))?;
            self.pos += 1;
            if b != 0xFF {
                continue;
            }
            while self.bytes.get(self.pos) == Some(&0xFF) {
                self.pos += 1;
            }
            let m = *self.bytes.get(self.pos).ok_or_else(|| JpegError::damaged("it ends before its picture does"))?;
            self.pos += 1;
            if m != 0 {
                return Ok(m);
            }
        }
    }

    /// The segment after a marker: its payload, past its length.
    fn segment(&mut self) -> Result<&'a [u8]> {
        let b = self.bytes.get(self.pos..self.pos + 2).ok_or_else(|| JpegError::damaged("a segment cut short"))?;
        let length = usize::from(u16::from_be_bytes([b[0], b[1]]));
        if length < 2 {
            return Err(JpegError::damaged("a segment shorter than its length"));
        }
        let payload =
            self.bytes.get(self.pos + 2..self.pos + length).ok_or_else(|| JpegError::damaged("a segment cut short"))?;
        self.pos += length;
        Ok(payload)
    }

    /// Read the file to its end, or with `header`, to its first scan.
    fn run(&mut self, header: bool) -> Result<()> {
        loop {
            let marker = match self.marker() {
                Ok(m) => m,
                // A file cut short is drawn as far as it goes.
                Err(_) if self.scans > 0 => return Ok(()),
                Err(e) => return Err(e),
            };
            match marker {
                0xD9 => {
                    if self.scans == 0 && !header {
                        return Err(JpegError::damaged("it ends before its first scan"));
                    }
                    return Ok(());
                }
                0xD8 => return Err(JpegError::damaged("a second start of image")),
                0x01 | 0xD0..=0xD7 => {}
                0xC0..=0xC2 => {
                    let payload = self.segment()?;
                    self.frame(payload, marker == 0xC2)?;
                }
                0xC3 | 0xC7 | 0xCB | 0xCF => {
                    return Err(JpegError("a lossless JPEG; save it as a baseline or progressive one".into()));
                }
                0xC5 | 0xC6 | 0xCD | 0xCE | 0xDE | 0xDF => {
                    return Err(JpegError("a hierarchical JPEG; save it as a baseline or progressive one".into()));
                }
                0xC9 | 0xCA | 0xCC => {
                    return Err(JpegError(
                        "a JPEG coded arithmetically; save it as a baseline or progressive one".into(),
                    ));
                }
                0xC4 => {
                    let payload = self.segment()?;
                    self.huffman(payload)?;
                }
                0xDB => {
                    let payload = self.segment()?;
                    self.quantization(payload)?;
                }
                0xDD => {
                    let payload = self.segment()?;
                    let b = payload.get(0..2).ok_or_else(|| JpegError::damaged("a restart interval cut short"))?;
                    self.restart = usize::from(u16::from_be_bytes([b[0], b[1]]));
                }
                0xDA => {
                    if header {
                        return Ok(());
                    }
                    let payload = self.segment()?;
                    self.scan(payload)?;
                }
                0xE0 => {
                    let payload = self.segment()?;
                    self.jfif |= payload.starts_with(b"JFIF\0");
                }
                0xE1 => {
                    let payload = self.segment()?;
                    if self.orientation == 1
                        && let Some(o) = exif_orientation(payload)
                    {
                        self.orientation = o;
                    }
                }
                0xEE => {
                    let payload = self.segment()?;
                    if payload.starts_with(b"Adobe") && payload.len() >= 12 {
                        self.adobe = Some(payload[11]);
                    }
                }
                _ => {
                    self.segment()?;
                }
            }
        }
    }

    fn frame(&mut self, p: &[u8], progressive: bool) -> Result<()> {
        if self.frame.is_some() {
            return Err(JpegError::damaged("two frames"));
        }
        let cut = || JpegError::damaged("a frame header cut short");
        let head = p.get(0..6).ok_or_else(cut)?;
        if head[0] != 8 {
            return Err(JpegError(format!("a JPEG of {} bits a sample; save it at 8, as photos are", head[0])));
        }
        let height = usize::from(u16::from_be_bytes([head[1], head[2]]));
        let width = usize::from(u16::from_be_bytes([head[3], head[4]]));
        if height == 0 {
            return Err(JpegError("a JPEG whose height follows its picture (DNL); save it again".into()));
        }
        if width == 0 {
            return Err(JpegError::damaged("a frame no pixels wide"));
        }
        let side = MAX_IMAGE_SIDE as usize;
        if width > side || height > side {
            return Err(JpegError(format!(
                "{width} × {height} px; images are at most {MAX_IMAGE_SIDE} px a side (SPEC §3.3)"
            )));
        }
        let n = usize::from(head[5]);
        match n {
            1 | 3 => {}
            4 => return Err(JpegError("a CMYK JPEG; save it as RGB".into())),
            _ => return Err(JpegError::damaged(&format!("a frame of {n} components"))),
        }
        let specs = p.get(6..6 + 3 * n).ok_or_else(cut)?;
        let mut components = Vec::with_capacity(n);
        for c in specs.as_chunks::<3>().0 {
            let (h, v) = (usize::from(c[1] >> 4), usize::from(c[1] & 15));
            if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
                return Err(JpegError::damaged("a component sampled outside 1 to 4"));
            }
            if c[2] > 3 {
                return Err(JpegError::damaged("a component names a quantization table past 3"));
            }
            if components.iter().any(|o: &Component| o.id == c[0]) {
                return Err(JpegError::damaged("two components with one id"));
            }
            components.push(Component {
                id: c[0],
                h,
                v,
                quant: usize::from(c[2]),
                latched: None,
                bw: 0,
                bh: 0,
                cw: 0,
                ch: 0,
                coefs: Vec::new(),
                plane: Vec::new(),
                pred: 0,
            });
        }
        // A frame of one component codes it a block at a time, whatever its factors say.
        if n == 1 {
            components[0].h = 1;
            components[0].v = 1;
        }
        let hmax = components.iter().map(|c| c.h).max().unwrap_or(1);
        let vmax = components.iter().map(|c| c.v).max().unwrap_or(1);
        if components.iter().any(|c| hmax % c.h != 0 || vmax % c.v != 0) {
            return Err(JpegError("a JPEG sampled at factors that do not divide the largest; save it again".into()));
        }
        let (mcux, mcuy) = (width.div_ceil(8 * hmax), height.div_ceil(8 * vmax));
        for c in &mut components {
            c.bw = mcux * c.h;
            c.bh = mcuy * c.v;
            c.cw = (width * c.h).div_ceil(hmax).div_ceil(8);
            c.ch = (height * c.v).div_ceil(vmax).div_ceil(8);
            if progressive {
                c.coefs = vec![0; c.bw * c.bh * 64];
            } else {
                c.plane = vec![0; c.bw * c.bh * 64];
            }
        }
        self.frame = Some(Frame { width, height, progressive, components, hmax, vmax, mcux, mcuy });
        Ok(())
    }

    fn huffman(&mut self, mut p: &[u8]) -> Result<()> {
        while !p.is_empty() {
            let cut = || JpegError::damaged("a Huffman table cut short");
            let class = p[0] >> 4;
            let id = usize::from(p[0] & 15);
            if class > 1 || id > 3 {
                return Err(JpegError::damaged("a Huffman table of no class or id a JPEG has"));
            }
            let counts: [u8; 16] = p.get(1..17).ok_or_else(cut)?.try_into().map_err(|_| cut())?;
            let total: usize = counts.iter().map(|&c| usize::from(c)).sum();
            if total > 256 {
                return Err(JpegError::damaged("a Huffman table of more than 256 symbols"));
            }
            let symbols = p.get(17..17 + total).ok_or_else(cut)?;
            let table = Huffman::new(&counts, symbols)?;
            if class == 0 {
                self.dc[id] = Some(table);
            } else {
                self.ac[id] = Some(table);
            }
            p = &p[17 + total..];
        }
        Ok(())
    }

    fn quantization(&mut self, mut p: &[u8]) -> Result<()> {
        while !p.is_empty() {
            let cut = || JpegError::damaged("a quantization table cut short");
            let wide = p[0] >> 4;
            let id = usize::from(p[0] & 15);
            if wide > 1 || id > 3 {
                return Err(JpegError::damaged("a quantization table of no precision or id a JPEG has"));
            }
            let size = if wide == 1 { 128 } else { 64 };
            let values = p.get(1..1 + size).ok_or_else(cut)?;
            let mut table = [0u16; 64];
            for (k, &z) in ZIGZAG.iter().enumerate() {
                table[usize::from(z)] = if wide == 1 {
                    u16::from_be_bytes([values[2 * k], values[2 * k + 1]])
                } else {
                    u16::from(values[k])
                };
            }
            self.quant[id] = Some(table);
            p = &p[1 + size..];
        }
        Ok(())
    }

    fn scan(&mut self, p: &[u8]) -> Result<()> {
        self.scans += 1;
        if self.scans > MAX_SCANS {
            return Err(JpegError::damaged(&format!("more than {MAX_SCANS} scans")));
        }
        let frame = self.frame.as_mut().ok_or_else(|| JpegError::damaged("a scan before its frame"))?;
        let cut = || JpegError::damaged("a scan header cut short");
        let n = usize::from(*p.first().ok_or_else(cut)?);
        if n == 0 || n > frame.components.len() {
            return Err(JpegError::damaged("a scan of no components, or more than its frame has"));
        }
        let specs = p.get(1..1 + 2 * n).ok_or_else(cut)?;
        let tail = p.get(1 + 2 * n..4 + 2 * n).ok_or_else(cut)?;
        let (ss, se, ah, al) =
            (usize::from(tail[0]), usize::from(tail[1]), u32::from(tail[2] >> 4), u32::from(tail[2] & 15));
        let mut scanned = Vec::with_capacity(n);
        for s in specs.as_chunks::<2>().0 {
            let i = frame
                .components
                .iter()
                .position(|c| c.id == s[0])
                .ok_or_else(|| JpegError::damaged("a scan names a component its frame does not have"))?;
            if scanned.iter().any(|&(j, _, _)| j == i) {
                return Err(JpegError::damaged("a scan names a component twice"));
            }
            let (td, ta) = (usize::from(s[1] >> 4), usize::from(s[1] & 15));
            if td > 3 || ta > 3 {
                return Err(JpegError::damaged("a scan names a Huffman table past 3"));
            }
            let c = &mut frame.components[i];
            if c.latched.is_none() {
                c.latched = Some(self.quant[c.quant].ok_or_else(|| {
                    JpegError::damaged("a component names a quantization table the file does not define")
                })?);
            }
            scanned.push((i, td, ta));
        }
        let progressive = frame.progressive;
        if progressive && (ss > se || se > 63 || (ss == 0 && se != 0) || (ss > 0 && n != 1) || al > 13 || ah > 13) {
            return Err(JpegError::damaged("a progressive scan out of order"));
        }
        let needs_dc = !progressive || (ss == 0 && ah == 0);
        let needs_ac = !progressive || ss > 0;
        let missing = || JpegError::damaged("a scan names a Huffman table the file does not define");
        let mut tables = Vec::with_capacity(n);
        for &(i, td, ta) in &scanned {
            let dc = if needs_dc { Some(self.dc[td].as_ref().ok_or_else(missing)?) } else { None };
            let ac = if needs_ac { Some(self.ac[ta].as_ref().ok_or_else(missing)?) } else { None };
            tables.push((i, dc, ac));
        }

        for c in &mut frame.components {
            c.pred = 0;
        }
        let interleaved = n > 1;
        let (across, down) = if interleaved {
            (frame.mcux, frame.mcuy)
        } else {
            let c = &frame.components[tables[0].0];
            (c.cw, c.ch)
        };
        let mut bits = Bits::new(self.bytes, self.pos);
        let mut eobrun = 0u32;
        for m in 0..across * down {
            if self.restart > 0 && m > 0 && m % self.restart == 0 {
                bits.restart();
                eobrun = 0;
                for c in &mut frame.components {
                    c.pred = 0;
                }
            }
            let (mx, my) = (m % across, m / across);
            for &(i, dc, ac) in &tables {
                let c = &mut frame.components[i];
                let (h, v) = if interleaved { (c.h, c.v) } else { (1, 1) };
                for by in 0..v {
                    for bx in 0..h {
                        let (x, y) = if interleaved { (mx * c.h + bx, my * c.v + by) } else { (mx, my) };
                        let at = y * c.bw + x;
                        if !progressive {
                            let quant = c.latched.as_ref().ok_or_else(missing)?;
                            let stride = c.bw * 8;
                            let out = &mut c.plane[y * 8 * stride + x * 8..];
                            sequential(
                                &mut bits,
                                dc.ok_or_else(missing)?,
                                ac.ok_or_else(missing)?,
                                &mut c.pred,
                                quant,
                                out,
                                stride,
                            )?;
                            continue;
                        }
                        let block = &mut c.coefs[at * 64..at * 64 + 64];
                        match (ss, ah) {
                            (0, 0) => {
                                let t = bits.decode(dc.ok_or_else(missing)?)?;
                                if t > 16 {
                                    return Err(JpegError::damaged("a DC difference of more than 16 bits"));
                                }
                                c.pred = c.pred.wrapping_add(bits.extend(u32::from(t)));
                                block[0] = (c.pred as i16).wrapping_mul(1 << al);
                            }
                            (0, _) => {
                                if bits.bit() {
                                    block[0] |= 1 << al;
                                }
                            }
                            (_, 0) => ac_first(&mut bits, ac.ok_or_else(missing)?, ss, se, al, &mut eobrun, block)?,
                            _ => ac_refine(&mut bits, ac.ok_or_else(missing)?, ss, se, al, &mut eobrun, block)?,
                        }
                    }
                }
            }
        }
        self.pos = bits.pos;
        Ok(())
    }

    /// The picture as stored: each component's samples, brought to the frame's resolution, colored
    /// as RGBA.
    fn pixels(&mut self) -> Result<Decoded> {
        let color = self.color();
        let frame = self.frame.as_mut().ok_or_else(|| JpegError::damaged("no frame"))?;
        if frame.progressive {
            for c in &mut frame.components {
                let quant = c.latched.unwrap_or([0; 64]);
                let stride = c.bw * 8;
                c.plane = vec![0; c.bw * c.bh * 64];
                let mut block = [0i32; 64];
                for y in 0..c.bh {
                    for x in 0..c.bw {
                        let at = (y * c.bw + x) * 64;
                        for (k, b) in block.iter_mut().enumerate() {
                            *b = i32::from(c.coefs[at + k]).wrapping_mul(i32::from(quant[k]));
                        }
                        idct(&mut block, &mut c.plane[y * 8 * stride + x * 8..], stride);
                    }
                }
                c.coefs = Vec::new();
            }
        }
        let (w, h) = (frame.width, frame.height);
        let mut rgba = vec![0u8; w * h * 4];
        let mut rows: Vec<Vec<i16>> = frame.components.iter().map(|_| vec![0; w]).collect();
        let mut between = vec![0i16; frame.components.iter().map(|c| c.bw * 8).max().unwrap_or(0)];
        for y in 0..h {
            for (c, row) in frame.components.iter().zip(rows.iter_mut()) {
                c.row(y, frame.hmax / c.h, frame.vmax / c.v, row, &mut between);
            }
            let out = &mut rgba[y * w * 4..(y + 1) * w * 4];
            match color {
                Color::Gray => {
                    for (px, &l) in out.as_chunks_mut::<4>().0.iter_mut().zip(&rows[0]) {
                        let l = l as u8;
                        *px = [l, l, l, 255];
                    }
                }
                Color::Rgb => {
                    for (x, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        *px = [rows[0][x] as u8, rows[1][x] as u8, rows[2][x] as u8, 255];
                    }
                }
                Color::YCbCr => {
                    for (x, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        *px = ycbcr(rows[0][x], rows[1][x], rows[2][x]);
                    }
                }
            }
        }
        Ok(Decoded { width: w as u32, height: h as u32, rgba })
    }

    /// How the components are colored, as libjpeg reads it: one is gray; three are YCbCr where a
    /// JFIF marker says so, RGB where an Adobe marker says they are not transformed, or with
    /// neither marker, where their ids spell R, G, B; and YCbCr otherwise.
    fn color(&self) -> Color {
        let Some(frame) = &self.frame else { return Color::Gray };
        if frame.components.len() == 1 {
            return Color::Gray;
        }
        let rgb_ids = frame.components.iter().map(|c| c.id).eq(*b"RGB");
        match self.adobe {
            _ if self.jfif => Color::YCbCr,
            Some(0) => Color::Rgb,
            Some(_) => Color::YCbCr,
            None if rgb_ids => Color::Rgb,
            None => Color::YCbCr,
        }
    }
}

impl Component {
    /// Row `y` of the frame, from this component sampled `hs` times fewer across and `vs` down:
    /// the triangle filter at 2, the nearest sample at any other ratio. The filter reaches into
    /// the component's whole blocks past the picture's edge, as zune-jpeg's does, and repeats
    /// the outermost sample of those.
    fn row(&self, y: usize, hs: usize, vs: usize, out: &mut [i16], between: &mut [i16]) {
        let stride = self.bw * 8;
        let last = self.bh * 8 - 1;
        let line = |r: usize| &self.plane[r * stride..(r + 1) * stride];
        // The nearer of row r's neighbors: the one above it for the frame's even rows, below for
        // its odd ones.
        let near = |r: usize| if y.is_multiple_of(2) { r.saturating_sub(1) } else { (r + 1).min(last) };
        let blend = |a: u8, b: u8| (3 * i16::from(a) + i16::from(b) + 2) >> 2;
        match (hs, vs) {
            (1, 1) => {
                for (o, &s) in out.iter_mut().zip(line(y)) {
                    *o = i16::from(s);
                }
            }
            (2, 1) => {
                for (t, &s) in between.iter_mut().zip(line(y)) {
                    *t = i16::from(s);
                }
                triangle(&between[..stride], out);
            }
            (1, 2) => {
                for ((o, &a), &b) in out.iter_mut().zip(line(y / 2)).zip(line(near(y / 2))) {
                    *o = blend(a, b);
                }
            }
            (2, 2) => {
                for ((t, &a), &b) in between.iter_mut().zip(line(y / 2)).zip(line(near(y / 2))) {
                    *t = blend(a, b);
                }
                triangle(&between[..stride], out);
            }
            _ => {
                let src = line(y / vs);
                for (x, o) in out.iter_mut().enumerate() {
                    *o = i16::from(src[x / hs]);
                }
            }
        }
    }
}

/// `input` at twice its width by the triangle filter, ¾ of the nearer sample and ¼ of the
/// farther, as far as `out` reaches.
fn triangle(input: &[i16], out: &mut [i16]) {
    let last = input.len() - 1;
    for (x, o) in out.iter_mut().enumerate() {
        let i = x / 2;
        let neighbor = if x.is_multiple_of(2) { i.saturating_sub(1) } else { (i + 1).min(last) };
        *o = (3 * input[i] + input[neighbor] + 2) >> 2;
    }
}

/// YCbCr to opaque RGBA: BT.601 at full range, 14-bit coefficients, as zune-jpeg converts.
fn ycbcr(y: i16, cb: i16, cr: i16) -> [u8; 4] {
    const Y: i32 = 16384;
    const CR: i32 = 22970;
    const CB: i32 = 29032;
    const G_CR: i32 = -11700;
    const G_CB: i32 = -5638;
    const ROUND: i32 = (1 << 13) - 1;
    let (cb, cr) = (i32::from(cb) - 128, i32::from(cr) - 128);
    let y = i32::from(y) * Y + ROUND;
    let clamp = |v: i32| (v >> 14).clamp(0, 255) as u8;
    [clamp(y + cr * CR), clamp(y + cr * G_CR + cb * G_CB), clamp(y + cb * CB), 255]
}

/// A block of a sequential scan, decoded, dequantized, and transformed into `out`, `stride`
/// samples a row.
fn sequential(
    bits: &mut Bits,
    dc: &Huffman,
    ac: &Huffman,
    pred: &mut i32,
    quant: &[u16; 64],
    out: &mut [u8],
    stride: usize,
) -> Result<()> {
    let mut block = [0i32; 64];
    let t = bits.decode(dc)?;
    if t > 16 {
        return Err(JpegError::damaged("a DC difference of more than 16 bits"));
    }
    *pred = pred.wrapping_add(bits.extend(u32::from(t)));
    block[0] = pred.wrapping_mul(i32::from(quant[0]));
    let mut k = 1;
    while k < 64 {
        let rs = bits.decode(ac)?;
        let (r, s) = (usize::from(rs >> 4), u32::from(rs & 15));
        if s == 0 {
            if r != 15 {
                break;
            }
            k += 16;
            continue;
        }
        k += r;
        if k > 63 {
            return Err(JpegError::damaged("a block runs past its 64 coefficients"));
        }
        let z = usize::from(ZIGZAG[k]);
        block[z] = bits.extend(s).wrapping_mul(i32::from(quant[z]));
        k += 1;
    }
    idct(&mut block, out, stride);
    Ok(())
}

/// A progressive scan's first pass over a band of AC coefficients, in one block.
fn ac_first(
    bits: &mut Bits,
    ac: &Huffman,
    ss: usize,
    se: usize,
    al: u32,
    eobrun: &mut u32,
    block: &mut [i16],
) -> Result<()> {
    if *eobrun > 0 {
        *eobrun -= 1;
        return Ok(());
    }
    let mut k = ss;
    while k <= se {
        let rs = bits.decode(ac)?;
        let (r, s) = (u32::from(rs >> 4), u32::from(rs & 15));
        if s == 0 {
            if r < 15 {
                *eobrun = (1 << r) - 1 + bits.bits(r);
                break;
            }
            k += 16;
            continue;
        }
        k += r as usize;
        if k > se {
            return Err(JpegError::damaged("a band runs past its last coefficient"));
        }
        block[usize::from(ZIGZAG[k])] = (bits.extend(s) as i16).wrapping_mul(1 << al);
        k += 1;
    }
    Ok(())
}

/// A progressive scan's later pass over a band of AC coefficients, in one block: a bit more of
/// each coefficient already there, and the ones that now are, as libjpeg refines them.
fn ac_refine(
    bits: &mut Bits,
    ac: &Huffman,
    ss: usize,
    se: usize,
    al: u32,
    eobrun: &mut u32,
    block: &mut [i16],
) -> Result<()> {
    let p1: i16 = 1 << al;
    let m1: i16 = -1 << al;
    // A coefficient already there takes a bit more of itself: its magnitude grows by `p1` where
    // the bit says so.
    let correct = |bits: &mut Bits<'_>, coef: &mut i16| {
        if bits.bit() && *coef & p1 == 0 {
            *coef = coef.wrapping_add(if *coef >= 0 { p1 } else { m1 });
        }
    };
    let mut k = ss;
    if *eobrun == 0 {
        while k <= se {
            let rs = bits.decode(ac)?;
            let (mut r, s) = (i32::from(rs >> 4), rs & 15);
            let mut value = 0;
            if s != 0 {
                value = if bits.bit() { p1 } else { m1 };
            } else if r != 15 {
                *eobrun = (1 << r) + bits.bits(r as u32);
                break;
            }
            // Past the coefficients already there, each with its correction bit, and r of those
            // not, to where the new one goes.
            while k <= se {
                let coef = &mut block[usize::from(ZIGZAG[k])];
                if *coef != 0 {
                    correct(bits, coef);
                } else {
                    if r == 0 {
                        break;
                    }
                    r -= 1;
                }
                k += 1;
            }
            if value != 0 && k <= se {
                block[usize::from(ZIGZAG[k])] = value;
            }
            k += 1;
        }
    }
    if *eobrun > 0 {
        while k <= se {
            let coef = &mut block[usize::from(ZIGZAG[k])];
            if *coef != 0 {
                correct(bits, coef);
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}

/// The inverse DCT of a dequantized block (natural order) into `out`, `stride` samples a row,
/// level-shifted and clamped to 0–255: stb_image's integer transform, as zune-jpeg's portable path
/// computes it, its arithmetic wrapping as theirs does.
fn idct(block: &mut [i32; 64], out: &mut [u8], stride: usize) {
    // A block of its DC term alone is that term's level everywhere: what the whole transform
    // gives it, sooner.
    if block[1..].iter().all(|&c| c == 0) {
        let dc = (block[0].wrapping_add(4).wrapping_add(1024) >> 3).clamp(0, 255) as u8;
        for row in 0..8 {
            out[row * stride..row * stride + 8].fill(dc);
        }
        return;
    }
    transform(block, out, stride);
}

/// The whole transform of [`idct`]: down each column, then along each row.
fn transform(block: &mut [i32; 64], out: &mut [u8], stride: usize) {
    const SCALE: i32 = 512 + 65536 + (128 << 17);
    let (wa, ws, wm) = (i32::wrapping_add, i32::wrapping_sub, i32::wrapping_mul);
    // Down each column.
    for c in 0..8 {
        let (p2, p3) = (block[c + 16], block[c + 48]);
        let p1 = wm(wa(p2, p3), 2217);
        let t2 = wa(p1, wm(p3, -7567));
        let t3 = wa(p1, wm(p2, 3135));
        let (p2, p3) = (block[c], block[c + 32]);
        let t0 = wa(p2, p3) << 12;
        let t1 = ws(p2, p3) << 12;
        let x0 = wa(wa(t0, t3), 512);
        let x3 = wa(ws(t0, t3), 512);
        let x1 = wa(wa(t1, t2), 512);
        let x2 = wa(ws(t1, t2), 512);
        let (o0, o1, o2, o3) = odd(block[c + 56], block[c + 40], block[c + 24], block[c + 8]);
        block[c] = wa(x0, o3) >> 10;
        block[c + 8] = wa(x1, o2) >> 10;
        block[c + 16] = wa(x2, o1) >> 10;
        block[c + 24] = wa(x3, o0) >> 10;
        block[c + 32] = ws(x3, o0) >> 10;
        block[c + 40] = ws(x2, o1) >> 10;
        block[c + 48] = ws(x1, o2) >> 10;
        block[c + 56] = ws(x0, o3) >> 10;
    }
    // Then along each row.
    for (r, row) in block.as_chunks::<8>().0.iter().enumerate() {
        let p1 = wm(wa(row[2], row[6]), 2217);
        let t2 = wa(p1, wm(row[6], -7567));
        let t3 = wa(p1, wm(row[2], 3135));
        let t0 = wa(row[0], row[4]) << 12;
        let t1 = ws(row[0], row[4]) << 12;
        let x0 = wa(wa(t0, t3), SCALE);
        let x3 = wa(ws(t0, t3), SCALE);
        let x1 = wa(wa(t1, t2), SCALE);
        let x2 = wa(ws(t1, t2), SCALE);
        let (o0, o1, o2, o3) = odd(row[7], row[5], row[3], row[1]);
        let clamp = |v: i32| (v >> 17).clamp(0, 255) as u8;
        out[r * stride..r * stride + 8].copy_from_slice(&[
            clamp(wa(x0, o3)),
            clamp(wa(x1, o2)),
            clamp(wa(x2, o1)),
            clamp(wa(x3, o0)),
            clamp(ws(x3, o0)),
            clamp(ws(x2, o1)),
            clamp(ws(x1, o2)),
            clamp(ws(x0, o3)),
        ]);
    }
}

/// The odd part of one pass of [`idct`], from the inputs at 7, 5, 3, and 1.
fn odd(i7: i32, i5: i32, i3: i32, i1: i32) -> (i32, i32, i32, i32) {
    let (wa, wm) = (i32::wrapping_add, i32::wrapping_mul);
    let p3 = wa(i7, i3);
    let p4 = wa(i5, i1);
    let p1 = wa(i7, i1);
    let p2 = wa(i5, i3);
    let p5 = wm(wa(p3, p4), 4816);
    let t0 = wm(i7, 1223);
    let t1 = wm(i5, 8410);
    let t2 = wm(i3, 12586);
    let t3 = wm(i1, 6149);
    let p1 = wa(p5, wm(p1, -3685));
    let p2 = wa(p5, wm(p2, -10497));
    let p3 = wm(p3, -8034);
    let p4 = wm(p4, -1597);
    (wa(t0, wa(p1, p3)), wa(t1, wa(p2, p4)), wa(t2, wa(p2, p3)), wa(t3, wa(p1, p4)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../tests/fixtures/jpeg/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// Each of the eight EXIF orientations turns a 3 × 2 picture as the EXIF specification
    /// draws it: pixels numbered 1–6 row by row, as stored, then as seen.
    #[test]
    fn each_orientation_turns_as_exif_says() {
        let stored = Decoded { width: 3, height: 2, rgba: (1..=6).flat_map(|v| [v, 0, 0, 255]).collect() };
        let seen = |o: u8| {
            let d = orient(stored.clone(), o);
            (d.width, d.height, d.rgba.as_chunks::<4>().0.iter().map(|p| p[0]).collect::<Vec<_>>())
        };
        assert_eq!(seen(1), (3, 2, vec![1, 2, 3, 4, 5, 6]));
        assert_eq!(seen(2), (3, 2, vec![3, 2, 1, 6, 5, 4]), "mirrored");
        assert_eq!(seen(3), (3, 2, vec![6, 5, 4, 3, 2, 1]), "turned half way");
        assert_eq!(seen(4), (3, 2, vec![4, 5, 6, 1, 2, 3]), "upside down");
        assert_eq!(seen(5), (2, 3, vec![1, 4, 2, 5, 3, 6]), "transposed");
        assert_eq!(seen(6), (2, 3, vec![4, 1, 5, 2, 6, 3]), "a quarter clockwise");
        assert_eq!(seen(7), (2, 3, vec![6, 3, 5, 2, 4, 1]), "transversed");
        assert_eq!(seen(8), (2, 3, vec![3, 6, 2, 5, 1, 4]), "a quarter anticlockwise");
    }

    /// The orientation is read from EXIF in either byte order, and nothing else is taken for it.
    #[test]
    fn the_orientation_is_read_from_exif_in_either_order() {
        let exif = |little: bool, tag: u16, kind: u16, value: u16| {
            type Order = (fn(u16) -> [u8; 2], fn(u32) -> [u8; 4]);
            let (h, w): Order =
                if little { (u16::to_le_bytes, u32::to_le_bytes) } else { (u16::to_be_bytes, u32::to_be_bytes) };
            let mut b = b"Exif\0\0".to_vec();
            b.extend_from_slice(if little { b"II*\0" } else { b"MM\0*" });
            b.extend(w(8));
            b.extend(h(1));
            b.extend(h(tag));
            b.extend(h(kind));
            b.extend(w(1));
            b.extend(h(value));
            b.extend([0, 0, 0, 0, 0, 0]);
            b
        };
        assert_eq!(exif_orientation(&exif(true, 0x0112, 3, 6)), Some(6));
        assert_eq!(exif_orientation(&exif(false, 0x0112, 3, 8)), Some(8));
        assert_eq!(exif_orientation(&exif(false, 0x0112, 3, 9)), None, "past 8");
        assert_eq!(exif_orientation(&exif(false, 0x0110, 3, 6)), None, "another tag");
        assert_eq!(exif_orientation(&exif(true, 0x0112, 4, 6)), None, "not a SHORT");
        assert_eq!(exif_orientation(&exif(true, 0x0112, 3, 6)[..20]), None, "cut short");
        assert_eq!(exif_orientation(b"http://ns.adobe.com/xap/1.0/\0"), None);
        // And the segment `stripped` writes says what it was given.
        assert_eq!(exif_orientation(&orientation_segment(7)[4..]), Some(7));
    }

    #[test]
    fn the_header_says_the_size_and_how_it_turns() {
        let h = Header::read(&fixture("orientation-6.jpg")).unwrap();
        assert_eq!((h.width, h.height, h.components, h.progressive, h.orientation), (67, 45, 3, false, 6));
        assert_eq!(h.size(), (45, 67));
        let p = Header::read(&fixture("gray-progressive.jpg")).unwrap();
        assert_eq!((p.components, p.progressive, p.orientation, p.size()), (1, true, 1, (67, 45)));
        let d = decode(&fixture("orientation-6.jpg")).unwrap();
        assert_eq!((d.width, d.height), (45, 67));
        assert_eq!(d.rgba, orient(decode_stored(&fixture("orientation-6.jpg")).unwrap(), 6).rgba);
    }

    /// `stripped` leaves out what a photo says beyond its picture and keeps what draws it: the
    /// same pixels, turned the same way.
    #[test]
    fn stripped_keeps_the_picture_and_drops_what_it_says() {
        let file = fixture("orientation-6.jpg");
        let bare = stripped(&file).unwrap();
        let has = |b: &[u8], s: &[u8]| b.windows(s.len()).any(|w| w == s);
        for said in [&b"Scaena Test Camera"[..], b"2026:10:06", b"taken somewhere", b"ICC_PROFILE", b"xmpmeta"] {
            assert!(has(&file, said) && !has(&bare, said), "{}", String::from_utf8_lossy(said));
        }
        assert!(bare.len() < file.len());
        assert_eq!(Header::read(&bare).unwrap().orientation, 6);
        assert_eq!(decode(&bare).unwrap(), decode(&file).unwrap());
        // A file with nothing to leave out keeps every byte that draws it; one with no
        // orientation gets none.
        for name in ["baseline-420.jpg", "progressive-444.jpg", "restart-progressive.jpg", "rgb.jpg"] {
            let file = fixture(name);
            let bare = stripped(&file).unwrap();
            assert_eq!(decode_stored(&bare).unwrap(), decode_stored(&file).unwrap(), "{name}");
            assert!(!has(&bare, b"Exif"), "{name}");
        }
        // A file cut short is stripped as far as it goes, and still ends as a JPEG ends.
        let cut = &file[..file.len() - 200];
        assert!(stripped(cut).unwrap().ends_with(&[0xFF, 0xD9]));
    }

    /// What the decoder cannot read is an error that says why.
    #[test]
    fn what_it_cannot_read_it_refuses_saying_why() {
        let refused = |bytes: &[u8]| decode(bytes).unwrap_err().to_string();
        assert!(refused(&fixture("cmyk.jpg")).contains("CMYK"));
        assert!(refused(b"\x89PNG\r\n\x1a\n").contains("not a JPEG"));
        // The frame of a baseline JPEG, marked lossless, arithmetic, at 12 bits, or with no height.
        let file = fixture("baseline-444.jpg");
        let sof = file.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        let with = |at: usize, byte: u8| {
            let mut b = file.clone();
            b[at] = byte;
            b
        };
        assert!(refused(&with(sof + 1, 0xC3)).contains("lossless"));
        assert!(refused(&with(sof + 1, 0xC9)).contains("arithmetic"));
        assert!(refused(&with(sof + 4, 12)).contains("12 bits"));
        let mut tall = file.clone();
        tall[sof + 5..sof + 7].copy_from_slice(&[0, 0]);
        assert!(refused(&tall).contains("DNL"));
        let mut huge = file.clone();
        huge[sof + 7..sof + 9].copy_from_slice(&9000u16.to_be_bytes());
        assert!(refused(&huge).contains("at most 8192 px a side"));
        assert!(Header::read(&huge).is_err(), "the header refuses it too");
    }

    /// A damaged file is drawn as far as it goes or refused, never a panic or a hang: every
    /// prefix of several files, and thousands of bytes changed at random.
    #[test]
    fn a_damaged_file_is_an_error_or_a_picture_never_a_panic() {
        for name in ["baseline-420.jpg", "progressive-420.jpg", "restart-420.jpg", "gray-progressive.jpg"] {
            let file = fixture(name);
            for end in 0..file.len() {
                let _ = decode(&file[..end]);
                let _ = Header::read(&file[..end]);
            }
        }
        // A seeded generator, so a failure is the same every run.
        let mut state: u64 = 0x5CAE_7A00_2066;
        let mut next = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as usize
        };
        let files: Vec<Vec<u8>> = [
            "baseline-422.jpg",
            "progressive-444.jpg",
            "restart-progressive.jpg",
            "sampled-411.jpg",
            "orientation-6.jpg",
        ]
        .iter()
        .map(|n| fixture(n))
        .collect();
        for _ in 0..3000 {
            let mut b = files[next() % files.len()].clone();
            for _ in 0..1 + next() % 4 {
                let at = next() % b.len();
                b[at] = next() as u8;
            }
            if let Ok(d) = decode(&b) {
                assert_eq!(d.rgba.len(), d.width as usize * d.height as usize * 4);
            }
            let _ = stripped(&b);
        }
    }

    /// A block with only its DC term is that term's level everywhere: the shortcut for it gives
    /// what the whole transform gives, for every DC term a JPEG of 8 bits can hold.
    #[test]
    fn a_flat_block_is_its_level_everywhere() {
        for dc in -2048..2048 {
            let (mut a, mut b) = ([0i32; 64], [0i32; 64]);
            (a[0], b[0]) = (dc, dc);
            let (mut short, mut whole) = ([0u8; 64], [0u8; 64]);
            idct(&mut a, &mut short, 8);
            transform(&mut b, &mut whole, 8);
            assert!(short.iter().all(|&v| v == short[0]), "{dc}");
            assert_eq!(short, whole, "{dc}");
        }
    }
}
