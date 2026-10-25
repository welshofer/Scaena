// A bundle's files in a directory (PLAN 2.4, SPEC §3.1): a folder on disk the page was given
// (File System Access), or one in the browser's own storage, the origin-private file system,
// which keeps bundles under `bundles/`. The same calls read and write both.

/** The browser's answer, once asked, whether it keeps what this page stores for good. */
let persisting: Promise<boolean> | undefined;

/** Ask the browser once to keep what this page stores, the decks it keeps and their recovery
 * copies (lift-up 10-24b REL-4): left best-effort, a browser short of room may clear them. Where it
 * will not, `say` is told once, with the way to keep a copy. Whether it keeps them. */
export function persist(say: (text: string) => void): Promise<boolean> {
  persisting ??= (async () => {
    // A page's own, which a worker's types, this file being in both, do not name.
    const storage = navigator.storage as StorageManager & { persist?: () => Promise<boolean> };
    if (!storage?.persist) return false;
    if (await storage.persisted().catch(() => false)) return true;
    const kept = await storage.persist().catch(() => false);
    if (!kept) say("This browser may clear the decks it keeps when it runs short of room: Download keeps a copy of your own.");
    return kept;
  })();
  return persisting;
}

/** Whether the browser keeps what this page stores for good, as it says now. */
export async function persisted(): Promise<boolean> {
  return (await navigator.storage?.persisted?.().catch(() => false)) ?? false;
}

/** Where the browser keeps bundles: `bundles/` in the origin-private file system. */
export async function kept(): Promise<FileSystemDirectoryHandle> {
  return (await navigator.storage.getDirectory()).getDirectoryHandle("bundles", { create: true });
}

/** The names of the bundles the browser keeps, sorted. */
export async function keptNames(): Promise<string[]> {
  const names: string[] = [];
  for await (const [name, handle] of (await kept()).entries()) if (handle.kind === "directory") names.push(name);
  return names.sort();
}

/** The bundle the browser keeps as `name`. */
export async function keptBundle(name: string): Promise<FileSystemDirectoryHandle> {
  return (await kept()).getDirectoryHandle(name).catch(() => {
    throw new Error(`the browser keeps no bundle named ${name}`);
  });
}

/** Why `name` cannot name a bundle the browser keeps, if it cannot: a folder's name in its
 * storage, which holds no slash and does not start with a dot, and one no other bundle has. */
export async function unkeepable(name: string): Promise<string | undefined> {
  if (!name) return "a deck needs a name";
  if (/[/\\]/.test(name) || name.startsWith(".")) return `“${name}” cannot name a deck: no slash, and not a dot first`;
  if ((await keptNames()).includes(name)) return `this browser keeps a deck named ${name} already`;
}

/** Rename the bundle the browser keeps as `from` to `to` (PLAN 2.124): every file of it written
 * under the new name, then the old folder taken away, so a rename that fails leaves it as it
 * was. */
export async function renameKept(from: string, to: string) {
  const refused = await unkeepable(to);
  if (refused) throw new Error(refused);
  const old = await keptBundle(from);
  const files: [string, Uint8Array][] = [];
  const walk = async (dir: FileSystemDirectoryHandle, prefix: string) => {
    for await (const [name, handle] of dir.entries()) {
      if (handle.kind === "directory") await walk(handle, `${prefix}${name}/`);
      else files.push([prefix + name, new Uint8Array(await (await handle.getFile()).arrayBuffer())]);
    }
  };
  await walk(old, "");
  const made = await (await kept()).getDirectoryHandle(to, { create: true });
  try {
    for (const [path, bytes] of files) await write(made, path, bytes);
  } catch (e) {
    await (await kept()).removeEntry(to, { recursive: true }).catch(() => {});
    throw e;
  }
  await (await kept()).removeEntry(from, { recursive: true });
}

/** Delete the bundle the browser keeps as `name`, and every file of it (PLAN 2.124). */
export async function deleteKept(name: string) {
  await keptBundle(name);
  await (await kept()).removeEntry(name, { recursive: true });
}

/** A new bundle in the browser's storage: `name`, or `name-2`, `name-3`, … where that is
 * taken. */
export async function newBundle(name: string): Promise<FileSystemDirectoryHandle> {
  const taken = new Set(await keptNames());
  let free = name;
  for (let i = 2; taken.has(free); i++) free = `${name}-${i}`;
  return (await kept()).getDirectoryHandle(free, { create: true });
}

/** Every file under `dir`, a bundle's folder, by its path inside it; names that start with a
 * dot are not the bundle's. */
export async function readAll(dir: FileSystemDirectoryHandle): Promise<Map<string, Uint8Array>> {
  await dir.getFileHandle("deck.json").catch(() => {
    throw new Error(`${dir.name} holds no deck.json: open a bundle's folder`);
  });
  const files = new Map<string, Uint8Array>();
  const walk = async (dir: FileSystemDirectoryHandle, prefix: string) => {
    for await (const [name, handle] of dir.entries()) {
      if (name.startsWith(".")) continue;
      if (handle.kind === "directory") await walk(handle, `${prefix}${name}/`);
      else files.set(prefix + name, new Uint8Array(await (await handle.getFile()).arrayBuffer()));
    }
  };
  await walk(dir, "");
  return files;
}

/** The directory `path` is in under `dir`, made if `make`. */
async function parent(dir: FileSystemDirectoryHandle, path: string, make: boolean) {
  const parts = path.split("/");
  const name = parts.pop()!;
  for (const part of parts) dir = await dir.getDirectoryHandle(part, { create: make });
  return { dir, name };
}

/** A worker's way into the origin-private file system where `createWritable` is missing. */
type SyncAccess = {
  truncate(size: number): void;
  write(bytes: Uint8Array, options: { at: number }): number;
  flush(): void;
  close(): void;
};

/** Write `bytes` at `path` under `dir`, making the directories it is in. */
export async function write(root: FileSystemDirectoryHandle, path: string, bytes: Uint8Array) {
  const { dir, name } = await parent(root, path, true);
  const file = await dir.getFileHandle(name, { create: true });
  if ("createWritable" in file) {
    const out = await file.createWritable();
    await out.write(bytes as Uint8Array<ArrayBuffer>);
    await out.close();
    return;
  }
  const out = await (file as unknown as { createSyncAccessHandle(): Promise<SyncAccess> }).createSyncAccessHandle();
  try {
    out.truncate(0);
    out.write(bytes, { at: 0 });
    out.flush();
  } finally {
    out.close();
  }
}

/** Remove the file at `path` under `dir`, if it is there. */
export async function remove(root: FileSystemDirectoryHandle, path: string) {
  try {
    const { dir, name } = await parent(root, path, false);
    await dir.removeEntry(name);
  } catch (e) {
    if (!(e instanceof DOMException && e.name === "NotFoundError")) throw e;
  }
}
