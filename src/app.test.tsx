import assert from "node:assert/strict";
import { test } from "node:test";
import { render } from "ink-testing-library";
import { App } from "./app.js";

test("shows the project root", () => {
  const { lastFrame, unmount } = render(<App root="/tmp/project" />);
  assert.match(lastFrame() ?? "", /\/tmp\/project/);
  unmount();
});
