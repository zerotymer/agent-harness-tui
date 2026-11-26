// Document downloads from the project configuration's `docs` (Stage 2 spec
// §6.3). Bytes are fetched over `https://`, checked against the optional
// `sha256` and saved by the apply engine under the references path.

import { sha256, TOOL_VERSION } from "./apply.js";
import type { DocsEntry } from "./catalog.js";

export const DOC_TIMEOUT_MS = 30_000;

export interface Downloaded {
  entry: DocsEntry;
  bytes: Buffer;
  /** `sha256:<hex>` of `bytes`. */
  hash: string;
  /** Set when the entry has no `sha256` to check against. */
  warning?: string;
}

export type Fetch = typeof fetch;

/** Downloads one document; throws an `Error` with a readable message. */
export async function download(entry: DocsEntry, fetchImpl: Fetch = fetch): Promise<Downloaded> {
  if (!entry.url.startsWith("https://")) throw new Error(`${entry.id}: only https:// URLs`);
  let resp: Response;
  try {
    resp = await fetchImpl(entry.url, {
      headers: { "User-Agent": `agent-harness-tui/${TOOL_VERSION}` },
      signal: AbortSignal.timeout(DOC_TIMEOUT_MS),
    });
  } catch (e) {
    const err = e as Error & { cause?: Error };
    throw new Error(`${entry.id}: download failed (${err.cause?.message ?? err.message})`);
  }
  if (!resp.ok) throw new Error(`${entry.id}: download failed (HTTP ${resp.status})`);
  const bytes = Buffer.from(await resp.arrayBuffer());
  const hash = sha256(bytes);
  if (entry.sha256 === undefined) {
    return {
      entry,
      bytes,
      hash,
      warning: `${entry.id}: no sha256 in the configuration; not verified`,
    };
  }
  if (hash !== `sha256:${entry.sha256.toLowerCase()}`) {
    throw new Error(`${entry.id}: sha256 mismatch (got ${hash.slice(7)}); not saved`);
  }
  return { entry, bytes, hash };
}
