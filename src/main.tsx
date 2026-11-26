#!/usr/bin/env node
// Entry point: runs the wizard in the current directory (the project root).

import { render } from "ink";
import { App } from "./app.js";
import { TOOL_VERSION } from "./apply.js";
import { ProjectRoot } from "./paths.js";
import { Wizard } from "./tui.js";

const USAGE =
  "usage: agent-harness-tui [--version | --help]\n" +
  "Run inside the project root. It never searches parent directories, initializes Git, commits or pushes.";

async function main(): Promise<number> {
  const arg = process.argv[2];
  if (arg === "--version" || arg === "-V") {
    console.log(`agent-harness-tui ${TOOL_VERSION}`);
    return 0;
  }
  if (arg === "--help" || arg === "-h") {
    console.log(USAGE);
    return 0;
  }
  if (arg !== undefined) {
    console.error(`unknown argument \`${arg}\`\n${USAGE}`);
    return 2;
  }
  // Fail fast instead of waiting when there is no interactive terminal.
  if (!process.stdin.isTTY || !process.stdout.isTTY) {
    console.error(
      "agent-harness-tui: an interactive terminal is required (stdin and stdout must be a TTY)",
    );
    return 2;
  }
  let root: ProjectRoot;
  try {
    root = ProjectRoot.fromCwd();
  } catch (e) {
    console.error(`agent-harness-tui: cannot use the current directory: ${(e as Error).message}`);
    return 1;
  }
  const wizard = await Wizard.create(root);
  // Ctrl+C is a wizard key: it cancels before anything is written.
  const app = render(<App wizard={wizard} />, { exitOnCtrlC: false, alternateScreen: true });
  await app.waitUntilExit();
  return 0;
}

process.exitCode = await main();
