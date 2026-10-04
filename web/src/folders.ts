// A bundle's files in a directory (PLAN 2.4, SPEC §3.1): a folder on disk the page was given
// (File System Access), or one in the browser's own storage, the origin-private file system,
// which keeps bundles under `bundles/`. The same calls read and write both.

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
