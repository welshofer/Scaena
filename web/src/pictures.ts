// A picture too large for a slide, offered as a smaller copy, not refused (SPEC §3.3): what a
// drop, a paste, or Insert takes into the bundle on the canvas and in the source. The engine
// reads its size as it is seen (`Stage.picture`); the browser makes the copy, upright, a PNG as a
// PNG with its transparency and a JPEG as a JPEG, and the file itself is left as it was. The Mac
// and the iPad offer the same with ImageIO (ScaenaKit's `Pasteboard.smaller`).
import type { Picture } from "./protocol";
import type { Stage } from "./stage";

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** `picture`'s size made to fit: its longer side the longest an image may have, the other in
 * proportion. */
export function fitted({ width, height, largest }: Picture): [number, number] {
  const longest = Math.max(width, height);
  if (longest <= largest) return [width, height];
  const scale = largest / longest;
  return [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
}

/** `file`, a PNG or a JPEG, as it goes into the bundle: as it is where an image shows it so; where
 * it is too large, a smaller copy, if the person wants one when asked. `undefined` where they want
 * none or none could be made, which `say` says. What the engine does not read as a picture goes on
 * as it is, for the drop to refuse with why. */
export async function fitting(stage: Stage, file: File, say: (words: string) => void): Promise<File | undefined> {
  const bytes = await file.arrayBuffer();
  let picture: Picture;
  try {
    picture = await stage.picture(bytes);
  } catch {
    return file;
  }
  const [w, h] = fitted(picture);
  if (w === picture.width && h === picture.height) return file;
  const n = (v: number) => v.toLocaleString();
  const asked =
    `${file.name} is ${n(picture.width)} × ${n(picture.height)} pixels, and a picture on a slide can be up to ` +
    `${n(picture.largest)} on a side. Add a copy ${n(w)} × ${n(h)} pixels in its place? The file itself isn’t changed.`;
  if (!confirm(asked)) {
    say(`${file.name} not added: ${n(picture.width)} × ${n(picture.height)} pixels is past ${n(picture.largest)} on a side`);
    return undefined;
  }
  say(`making a copy of ${file.name}, ${n(w)} × ${n(h)} pixels…`);
  try {
    return await smaller(file, isPng(bytes), w, h);
  } catch (e) {
    say(`${file.name} not added: no smaller copy of it could be made (${said(e)})`);
    return undefined;
  }
}

/** Whether `bytes` begin as a PNG does. */
function isPng(bytes: ArrayBuffer): boolean {
  const head = new Uint8Array(bytes, 0, Math.min(8, bytes.byteLength));
  return [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a].every((b, i) => head[i] === b);
}

/** A copy of `file` `w` × `h` pixels, upright as it is seen, under the same name: a PNG as a PNG,
 * its transparency kept, and a JPEG as a JPEG. */
export async function smaller(file: File, png: boolean, w: number, h: number): Promise<File> {
  const bitmap = await createImageBitmap(file, { imageOrientation: "from-image", resizeWidth: w, resizeHeight: h, resizeQuality: "high" });
  try {
    const canvas = new OffscreenCanvas(w, h);
    const context = canvas.getContext("2d");
    if (!context) throw new Error("the browser gave no canvas to draw it on");
    // A clear canvas, so what is clear in a PNG stays clear.
    context.drawImage(bitmap, 0, 0);
    const blob = await canvas.convertToBlob(png ? { type: "image/png" } : { type: "image/jpeg", quality: 0.92 });
    return new File([blob], file.name, { type: blob.type });
  } finally {
    bitmap.close();
  }
}
