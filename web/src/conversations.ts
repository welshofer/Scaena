// The assistant's conversation about a deck, kept in this browser (lift-up 20261110 FUNC-4): after
// each question, the conversation as the model is next sent it, every picture left out, and what
// the panel showed of it, the questions and the answers, in the browser's own storage (the
// origin-private file system) under `conversations/`, beside the recovery copies. Each deck's is a
// file named by the SHA-256 of the key its recovery copy goes by, so a reload, or the deck opened
// again tomorrow, offers to go on with it. New conversation takes it away.
import { sha256 } from "./recovery";

/** What a deck's conversation keeps. */
export interface Kept {
  /** When it was kept, ms since the epoch. */
  at: number;
  /** The conversation, in no provider's form, as the worker holds it. */
  messages: unknown[];
  /** What the panel showed of it: each question and each answer, in order. */
  said: { who: "user" | "said"; text: string }[];
}

const DIR = "conversations";

async function file(key: string, create: boolean): Promise<FileSystemFileHandle | null> {
  try {
    const root = await navigator.storage.getDirectory();
    const all = await root.getDirectoryHandle(DIR, { create });
    return await all.getFileHandle(`${await sha256(new TextEncoder().encode(key))}.json`, { create });
  } catch {
    return null;
  }
}

/** Keep `kept` as the deck `key`'s conversation; a browser that keeps nothing keeps none. */
export async function keep(key: string, kept: Kept) {
  const handle = await file(key, true);
  if (!handle) return;
  const out = await handle.createWritable();
  await out.write(JSON.stringify(kept));
  await out.close();
}

/** The deck `key`'s conversation, where one is kept and reads. */
export async function find(key: string): Promise<Kept | null> {
  const handle = await file(key, false);
  if (!handle) return null;
  try {
    const kept = JSON.parse(await (await handle.getFile()).text()) as Kept;
    return Array.isArray(kept.messages) && kept.messages.length && Array.isArray(kept.said) ? kept : null;
  } catch {
    return null;
  }
}

/** Take the deck `key`'s conversation away. */
export async function discard(key: string) {
  try {
    const root = await navigator.storage.getDirectory();
    const all = await root.getDirectoryHandle(DIR);
    await all.removeEntry(`${await sha256(new TextEncoder().encode(key))}.json`);
  } catch {
    // None was kept.
  }
}
