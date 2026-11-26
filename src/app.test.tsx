import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import type { Key as InkKey } from "ink";
import { render } from "ink-testing-library";
import { App, toKey } from "./app.js";
import { ProjectRoot } from "./paths.js";
import { Wizard } from "./tui.js";

test("maps terminal keys", () => {
  const k = (over: Partial<InkKey>) => ({ ...({} as InkKey), ...over });
  assert.deepEqual(toKey("c", k({ ctrl: true })), { name: "ctrl-c" });
  assert.deepEqual(toKey("", k({ return: true })), { name: "enter" });
  assert.deepEqual(toKey("", k({ delete: true })), { name: "backspace" });
  assert.deepEqual(toKey("참", k({})), { name: "char", ch: "참" });
  assert.equal(toKey("x", k({ ctrl: true })), undefined);
});

test("renders the wizard at small and large sizes", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aht-app-"));
  try {
    const w = await Wizard.create(new ProjectRoot(dir));
    for (const columns of [20, 60, 140]) {
      Object.defineProperty(process.stdout, "columns", { value: columns, configurable: true });
      const { lastFrame, unmount } = render(<App wizard={w} />);
      const frame = lastFrame() ?? "";
      assert.ok(frame.includes("루트·설정"), frame);
      if (columns >= 80) assert.ok(frame.includes("▸ 1 루트·설정"), frame);
      unmount();
    }
    assert.equal(fs.readdirSync(dir).length, 0, "rendering writes nothing");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
