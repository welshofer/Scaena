# ADR-0017: Photos are JPEGs, decoded in integers the same on every target

**Status:** proposed · **Date:** 2026-10-06

## Context

v1 reads PNG only (SPEC §3.3). A photo has to be converted before it enters a bundle, and SPEC §16 Q11 left JPEG open: "its IDCT must be checked for bit-exactness across SIMD paths before it can join the goldens". The editor's road (ADR-0013) takes a photo dropped on the canvas, and people's photos are JPEGs. As PNGs they are five to ten times larger, and so are the bundle and the single file that carry them.

Five facts frame the choice:
- **A frame is the same bytes on every target** (SPEC §13): the CPU painter natively and in the browser, and the GPU painters within §13.5's tolerance. A photo's pixels are part of the frame.
- **zune-jpeg is already in the build, and it picks its path by the processor.** krilla reads a JPEG's header with it, `image` decodes with it, and hayro does in tests.
  - It chooses its IDCT, color conversion, and chroma upsampling at run time: AVX2 on x86-64, NEON on arm64, and its portable path elsewhere. In the browser it always takes the portable path.
  - Nothing turns the vector paths off. `use_avx2()` is true when either its unsafe flag or its AVX2 flag is, and no public setter clears both. Its `x86` and `neon` features are on in every build, because krilla and `image` ask for its defaults.
- **Its portable path is integer arithmetic.** It uses stb_image's IDCT in 32-bit integers that wrap, BT.601 YCbCr in 14-bit fixed point, and a triangle filter for chroma. Integer code compiles to the same bytes' worth of results on every target, however the compiler vectorizes it.
- **A JPEG says more than its picture.** It can carry where and when it was taken, the camera's serial number, a thumbnail, XMP, a color profile, and comments.
  - It also carries an EXIF orientation: how the stored pixels turn to be seen. Phones store a portrait photo on its side.
- **Exports differ in what they carry.**
  - PNG, SVG, and video carry pixels the painters drew.
  - A PDF carries images as files its viewer decodes, as it rasterizes the PDF's text with its own rasterizer.

## Decision

1. **`scaena_core::jpeg` decodes JPEG with integers alone.** One source serves every target, and no path is chosen at run time. Its arithmetic is zune-jpeg's portable path, so it gives that path's bytes:
   - **Reads** baseline, extended, and progressive Huffman-coded JPEGs at 8 bits a sample. That covers gray, and three components that are YCbCr, or RGB where an Adobe marker or the components' ids say so. Any sampling whose factors divide the largest is read, with or without restart intervals.
   - **A file cut short** is drawn as far as it goes, as libjpeg draws it.
   - **Refuses, saying why:** arithmetic coding, lossless and hierarchical JPEGs, 12 bits a sample, CMYK, a height that only a DNL marker gives, and more than 8192 px a side.
   - The decoder is about 1,100 lines of code; its IDCT is 80 of them.
2. **It is held to zune-jpeg, file by file.**
   - `photos_decode_as_zune_jpeg_does` (in `scaena-export`, through `image`) compares every file in `tests/fixtures/jpeg` byte for byte. There are 17, covering:
     - sampling at 4:4:4, 4:2:2, 4:2:0, 4:1:1, and 4:4:0;
     - progressive scans, with restarts and without, and Huffman tables of a file's own;
     - gray and RGB;
     - sizes that are whole blocks and sizes that are not, and 1 × 1.
   - zune-jpeg takes its AVX2 paths on the machines CI runs on. So the test passing also says those paths agree with its portable one on these files.
   - Torture case 50 holds seven of the files to goldens: natively, in the WASM smoke page, and in the web player.
3. **The EXIF orientation is applied when the file is decoded.**
   - An image's size and its pixels are the picture as it is seen, and `fit`, `crop`, and `focal` speak of that picture.
   - The engine reads the orientation from the header, for the size it lays out. The painters apply it to the pixels. One parser serves both.
4. **The bundle keeps the file as it is.** It is saved as `assets/<sha256>.jpg`, and its content id is its bytes. Exports that carry pixels draw the decoded picture.
   - **The PDF carries the file itself,** drawn through the orientation's turn. It keeps what draws the picture: the frame, its tables and scans, and the JFIF and Adobe markers.
   - It leaves out EXIF, XMP, IPTC, the color profile, comments, and whatever follows the picture's end (`jpeg::stripped`). A photo in a PDF says nothing of where it was taken.
5. **Color profiles are still not applied** (SPEC §3.3): pixels are taken as sRGB.

## Consequences

- **+** A photo goes in as it is. It can be dropped on the canvas or on the source, named in a patch, or inserted from Insert and the Files panel.
- **+** No dependency is added. The editor's module grows by the decoder's code alone (PLAN 2.66 records the size).
- **+** A photo in a PDF is its own JPEG: a fraction of its pixels' size, and nothing of its metadata.
- **−** The decoder is ours to keep, and so are its bugs.
  - `a_damaged_file_is_an_error_or_a_picture_never_a_panic` holds it to never panicking: every truncation of four files, and 3,000 files with bytes changed at random.
  - The zune-jpeg test holds it to someone else's bytes.
- **−** CMYK and 12-bit JPEGs are refused. A print shop's CMYK file has to be converted first.
- **−** vello's image atlas is unchanged. It is 8192 px square and holds every image a frame draws, so photos make the case Q11 raised likelier: six 12-megapixel photos on one slide. Reducing each image to the size it is drawn at is its own task (SPEC §16 Q11).

## Alternatives

- **zune-jpeg in the painters, held by goldens.** Its vector paths agree with its portable one on our files today, but nothing pins that. A later version's AVX2 path could round differently, and only a machine without AVX2, a Mac, or a browser would see it. Nothing in its options turns the vector paths off.
- **`jpeg-decoder`.** It also picks SSE or NEON paths at run time, and it would be a new dependency.
- **Convert to PNG when a photo enters the bundle.** The bundle and the single file grow several times over, and the file in the bundle is no longer the one its owner gave.
- **Decode with the browser's own decoder (`createImageBitmap`).** It differs from browser to browser, and the CLI would need another decoder.
