// The user's API keys, in the page (PLAN 2.6, SPEC §11, ADR-0006). A key is kept for the tab
// alone by default (`sessionStorage`), gone when the tab closes. Kept on this device, it is
// encrypted (AES-GCM) with a key the browser keeps in IndexedDB and never hands out: a copy of
// the page's storage does not give it away, but any script this page runs can still use it,
// which the page says. A key is never written into a bundle, and goes nowhere but to the
// provider it is for.
import type { ProviderId } from "../protocol";

const item = (provider: ProviderId) => `scaena.key.${provider}`;

/** The key that encrypts the keys kept on this device: made once, kept in IndexedDB, never
 * extractable. */
function vault(): Promise<CryptoKey> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open("scaena-keys", 1);
    open.onupgradeneeded = () => open.result.createObjectStore("vault");
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const db = open.result;
      const got = db.transaction("vault").objectStore("vault").get("aes");
      got.onerror = () => reject(got.error);
      got.onsuccess = async () => {
        if (got.result) return resolve(got.result as CryptoKey);
        const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
        const put = db.transaction("vault", "readwrite").objectStore("vault").put(key, "aes");
        put.onerror = () => reject(put.error);
        put.onsuccess = () => resolve(key);
      };
    };
  });
}

const base64 = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));
const bytesOf = (text: string) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0));

/** The key for `provider`, and whether it is kept on this device; none if there is none. */
export async function load(provider: ProviderId): Promise<{ key: string; kept: boolean } | undefined> {
  const session = sessionStorage.getItem(item(provider));
  if (session) return { key: session, kept: false };
  const stored = localStorage.getItem(item(provider));
  if (!stored) return undefined;
  try {
    const { iv, data } = JSON.parse(stored) as { iv: string; data: string };
    const plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: bytesOf(iv) }, await vault(), bytesOf(data));
    return { key: new TextDecoder().decode(plain), kept: true };
  } catch {
    // Kept by another vault, which this browser no longer has: forget it.
    localStorage.removeItem(item(provider));
    return undefined;
  }
}

/** Keep `key` for `provider`: for this tab, or on this device, encrypted. */
export async function store(provider: ProviderId, key: string, kept: boolean) {
  forget(provider);
  if (!kept) return sessionStorage.setItem(item(provider), key);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const data = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv }, await vault(), new TextEncoder().encode(key)));
  localStorage.setItem(item(provider), JSON.stringify({ iv: base64(iv), data: base64(data) }));
}

/** Forget the key for `provider`, wherever it is kept. */
export function forget(provider: ProviderId) {
  sessionStorage.removeItem(item(provider));
  localStorage.removeItem(item(provider));
}

/** What the page remembers of the assistant's settings, none of it secret: the provider, and
 * for each provider the model picked and the address it is called at. */
export interface Settings {
  provider: ProviderId;
  models: Partial<Record<ProviderId, string>>;
  bases: Partial<Record<ProviderId, string>>;
}

export function settings(): Settings {
  try {
    const saved = JSON.parse(localStorage.getItem("scaena.assistant") ?? "{}") as Partial<Settings>;
    return { provider: saved.provider ?? "anthropic", models: saved.models ?? {}, bases: saved.bases ?? {} };
  } catch {
    return { provider: "anthropic", models: {}, bases: {} };
  }
}

export function keep(s: Settings) {
  try {
    localStorage.setItem("scaena.assistant", JSON.stringify(s));
  } catch {
    // Storage refused: the settings last for this page.
  }
}
