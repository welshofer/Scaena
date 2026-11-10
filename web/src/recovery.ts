// A recovery copy of what the editor has not saved (lift-up REL-4): the source as it stands, and
// the bundle's files written since the last save, kept in the browser's own storage (the
// origin-private file system) under `recovery/`, beside the `bundles/` it keeps. The editor
// writes it a few seconds after each change, and when its page is hidden, and takes it away with
// a save. A deck that opens with a copy is offered it: Recover, or Discard.
//
// Each deck's copy is a directory named by the SHA-256 of its key, where it is kept or where it
// came from: `source.scn`, the source; `files/`, each file written since the save, at its path;
// and `copy.json`, written last, which says when, and which files are in it and which were taken
// out. A copy without it is no copy: a write that did not finish is passed over.

/** What a copy holds. */
export interface Copy {
  /** When it was written, ms since the epoch. */
  at: number;
  source: string;
  /** The deck's name, as the editor had it. */
  name: string;
  /** The files written since the save, by their paths in the bundle, and those taken out. */
  files: string[];
  removed: string[];
  /** How to open the deck again, for a deck kept nowhere, which nothing else opens again: a new
   * deck by its theme and title, or a deck of files by the files it opened with, which the copy
   * carries under `bundle/`, at their paths. */
  reopen?: Reopen;
  /** What names it, kept with it, so a list of every copy can find each. */
  key?: string;
}

/** How a deck kept nowhere opens again. */
export type Reopen = { create: { theme: string; title: string } } | { files: string[] };

/** The record `copy.json` holds. */
type Record_ = Omit<Copy, "source">;

const DIR = "recovery";

/** The directory of the copy for `key`, made if `create`; null where there is none. */
async function dir(key: string, create: boolean): Promise<FileSystemDirectoryHandle | null> {
  const root = await navigator.storage.getDirectory();
  const all = await root.getDirectoryHandle(DIR, { create });
  return all.getDirectoryHandle(await hashed(key), { create }).catch(() => null);
}

/** `key` as a name in the store: its SHA-256, in hex. */
async function hashed(key: string): Promise<string> {
  return sha256(new TextEncoder().encode(key));
}

/** `bytes`' SHA-256, in hex: what tells a deck kept nowhere from another of its name. */
export async function sha256(bytes: ArrayBuffer | Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The handle of the file at `path` under `root`, making its directories if `create`. */
async function fileAt(root: FileSystemDirectoryHandle, path: string, create: boolean) {
  const parts = path.split("/");
  const name = parts.pop()!;
  let at = root;
  for (const part of parts) at = await at.getDirectoryHandle(part, { create });
  return at.getFileHandle(name, { create });
}

async function write(root: FileSystemDirectoryHandle, path: string, bytes: Uint8Array | string) {
  const out = await (await fileAt(root, path, true)).createWritable();
  await out.write(typeof bytes === "string" ? bytes : (bytes as Uint8Array<ArrayBuffer>));
  await out.close();
}

async function read(root: FileSystemDirectoryHandle, path: string): Promise<File | null> {
  return (await fileAt(root, path, false).catch(() => null))?.getFile() ?? null;
}

/** Keep `copy` for `key`: its source, each of `files` not yet kept, the files `bundle` holds,
 * which open its deck again, and the record, last. A copy kept before keeps the files it holds.
 * Whether the bundle was kept: one the browser has no room for is taken out again, and the copy is
 * kept without it, as one that cannot open its deck again. */
export async function keep(key: string, copy: Copy, files: Map<string, Uint8Array>, bundle?: Record<string, ArrayBuffer>): Promise<boolean> {
  const at = (await dir(key, true))!;
  await write(at, "source.scn", copy.source);
  for (const [path, bytes] of files) await write(at, `files/${path}`, bytes);
  let carried = true;
  if (bundle) {
    try {
      for (const [path, bytes] of Object.entries(bundle)) await write(at, `bundle/${path}`, new Uint8Array(bytes));
    } catch {
      carried = false;
      await at.removeEntry("bundle", { recursive: true }).catch(() => {});
    }
  }
  const record: Record_ = { at: copy.at, name: copy.name, files: copy.files, removed: copy.removed, reopen: carried ? copy.reopen : undefined, key };
  await write(at, "copy.json", JSON.stringify(record));
  return carried;
}

/** How long a copy nothing will offer again is kept, ms: a week. */
const STRANDED = 7 * 24 * 60 * 60 * 1000;

/** Every copy that opens its deck again, a deck kept nowhere's, the latest first: what a page left
 * that nothing else would offer. A copy of a deck kept nowhere that cannot open it again, as those
 * kept before a copy said how, and no copy of a deck kept somewhere, is taken away a week on: no
 * deck opened will find it. */
export async function leftovers(now = Date.now()): Promise<(Record_ & { key: string; reopen: Reopen; title?: string })[]> {
  const root = await navigator.storage.getDirectory();
  const all = await root.getDirectoryHandle(DIR).catch(() => null);
  const out: (Record_ & { key: string; reopen: Reopen; title?: string })[] = [];
  if (!all) return out;
  const stranded: string[] = [];
  for await (const [name, handle] of all.entries()) {
    if (handle.kind !== "directory") continue;
    const at = handle as FileSystemDirectoryHandle;
    const record = await read(at, "copy.json");
    try {
      const parsed = record && (JSON.parse(await record.text()) as Record_);
      if (!parsed) continue;
      const nowhere = !parsed.key || parsed.key.startsWith("files:") || parsed.key.startsWith("new:");
      if (!parsed.reopen && nowhere && now - parsed.at > STRANDED) stranded.push(name);
      if (!parsed.key || !parsed.reopen) continue;
      // Its title, as its source's `deck` line writes it: what a person calls it.
      const source = await (await read(at, "source.scn"))?.text();
      const header = source && /^deck\s+("(?:[^"\\\n]|\\.)*")/m.exec(source);
      const title = header ? (JSON.parse(header[1]) as string) : undefined;
      out.push({ ...parsed, key: parsed.key, reopen: parsed.reopen, title });
    } catch {
      // A record that does not read is no copy.
    }
  }
  for (const name of stranded) await all.removeEntry(name, { recursive: true }).catch(() => {});
  return out.sort((a, b) => b.at - a.at);
}

/** The files a copy's deck opened with, by their paths: what opens it again. */
export async function bundleOf(key: string, paths: string[]): Promise<Record<string, ArrayBuffer>> {
  const at = await dir(key, false);
  const out: Record<string, ArrayBuffer> = {};
  for (const path of paths) {
    const file = at && (await read(at, `bundle/${path}`));
    if (!file) throw new Error(`the copy lacks ${path}`);
    out[path] = await file.arrayBuffer();
  }
  return out;
}

/** The copy kept for `key`, with a reader of each file it holds; null where there is none whole. */
export async function find(key: string): Promise<(Copy & { file: (path: string) => Promise<Uint8Array | null> }) | null> {
  const at = await dir(key, false);
  if (!at) return null;
  const record = await read(at, "copy.json");
  const source = await read(at, "source.scn");
  if (!record || !source) return null;
  try {
    const parsed = JSON.parse(await record.text()) as Record_;
    return {
      ...parsed,
      source: await source.text(),
      file: async (path) => {
        const file = await read(at, `files/${path}`);
        return file ? new Uint8Array(await file.arrayBuffer()) : null;
      },
    };
  } catch {
    return null;
  }
}

/** Take away the copy kept for `key`, if any. */
export async function discard(key: string) {
  const root = await navigator.storage.getDirectory();
  const all = await root.getDirectoryHandle(DIR).catch(() => null);
  await all?.removeEntry(await hashed(key), { recursive: true }).catch(() => {});
}

/** When the bundle's deck was last saved where `dir` keeps it, ms since the epoch; none where it
 * cannot be read. A copy older than the deck is passed over: the deck was saved after it, by
 * another page or the CLI. */
export async function savedAt(dir: FileSystemDirectoryHandle | undefined): Promise<number | undefined> {
  if (!dir) return undefined;
  return (await dir.getFileHandle("deck.json").then((h) => h.getFile()).catch(() => null))?.lastModified;
}
