import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";
import type { DocsEntry } from "./catalog.js";
import { download, type Fetch } from "./docs.js";

const body = "# Doc\n";
const hex = createHash("sha256").update(body).digest("hex");
const entry = (extra: Partial<DocsEntry> = {}): DocsEntry =>
  ({
    id: "d",
    title: { en: "D" },
    url: "https://example.test/d.md",
    filename: "d.md",
    ...extra,
  }) as DocsEntry;
const serve =
  (status = 200): Fetch =>
  async () =>
    new Response(body, { status });

test("checks sha256 when given and warns without it", async () => {
  const ok = await download(entry({ sha256: hex.toUpperCase() }), serve());
  assert.equal(ok.bytes.toString(), body);
  assert.equal(ok.hash, `sha256:${hex}`);
  assert.equal(ok.warning, undefined);
  assert.match((await download(entry(), serve())).warning ?? "", /not verified/);
  await assert.rejects(download(entry({ sha256: "0".repeat(64) }), serve()), /mismatch.*not saved/);
});

test("rejects non-https URLs and failed responses", async () => {
  await assert.rejects(download(entry({ url: "http://example.test/d.md" }), serve()), /https/);
  await assert.rejects(download(entry(), serve(404)), /HTTP 404/);
  const down: Fetch = async () => {
    throw new TypeError("fetch failed", { cause: new Error("ECONNREFUSED") });
  };
  await assert.rejects(download(entry(), down), /ECONNREFUSED/);
});
