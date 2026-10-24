// A body read no further than it may be (the 20261024 plan's SEC-2): a source a person names, or
// a redirect it takes, can stream gigabytes, and a page that read the whole before it measured it
// would run out of memory first. Read as it arrives, it stops one chunk past `most`. The CLI reads
// the same plan's `most` the same way (`crates/scaena-cli/src/sources.rs`).

/** `response`'s body, `most` bytes at most; past them, it stops reading and says so for `url`. */
export async function atMost(response: Response, most: number, url: string): Promise<Uint8Array> {
  const tooMuch = () => new Error(`${url} answered with more than ${most >> 20} MB`);
  const said = Number(response.headers.get("content-length"));
  if (Number.isFinite(said) && said > most) {
    await response.body?.cancel();
    throw tooMuch();
  }
  if (!response.body) return new Uint8Array(0);
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let read = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    read += value.length;
    if (read > most) {
      await reader.cancel();
      throw tooMuch();
    }
    chunks.push(value);
  }
  const body = new Uint8Array(read);
  let at = 0;
  for (const chunk of chunks) {
    body.set(chunk, at);
    at += chunk.length;
  }
  return body;
}
