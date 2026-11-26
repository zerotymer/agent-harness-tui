import { Box, type Key as InkKey, Text, useApp, useInput, useWindowSize } from "ink";
import { useEffect, useReducer } from "react";
import * as install from "./install.js";
import { type Key, STEPS, type Wizard } from "./tui.js";

/** Maps Ink's key report to the wizard's terminal-independent key. */
export function toKey(input: string, key: InkKey): Key | undefined {
  if (key.ctrl && input === "c") return { name: "ctrl-c" };
  if (key.return) return { name: "enter" };
  if (key.escape) return { name: "esc" };
  if (key.upArrow) return { name: "up" };
  if (key.downArrow) return { name: "down" };
  if (key.pageUp) return { name: "pageup" };
  if (key.pageDown) return { name: "pagedown" };
  // Many terminals send DEL for Backspace, which Ink reports as `delete`.
  if (key.backspace || key.delete) return { name: "backspace" };
  if (key.ctrl || key.meta || input === "") return undefined;
  return { name: "char", ch: input };
}

/** Draws the wizard and feeds it keys. All state lives in `Wizard`. */
export function App({ wizard: w }: { wizard: Wizard }) {
  const { exit, suspendTerminal } = useApp();
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const { columns, rows } = useWindowSize();

  // Detect agents after the first draw so the screen never waits for it.
  useEffect(() => {
    if (w.step === "agents") void w.detect().then(redraw);
  }, [w]);

  useInput((input, key) => {
    const k = toKey(input, key);
    if (!k) return;
    void (async () => {
      const pending = w.key(k);
      redraw(); // show the busy state while a fetch runs
      await pending;
      const run = w.takeRun();
      if (run) {
        // Hand the terminal over so the commands' prompts reach the user,
        // one command after another, then wait for Enter before redrawing.
        const results: Parameters<Wizard["finishRun"]>[1] = [];
        await suspendTerminal(async () => {
          for (const req of run.requests) {
            console.log(`\n$ ${install.display(req)}`);
            try {
              results.push({ ok: true, value: install.run(req, w.root.path) });
            } catch (e) {
              results.push({ ok: false, error: (e as Error).message });
            }
          }
          await install.waitForEnter();
        });
        w.busy = true;
        redraw();
        await w.finishRun(run, results);
        w.busy = false;
      }
      if (w.quit) exit();
      else redraw();
    })();
  });

  const current = w.stepIndex();
  // Header (1) + footer (3) + body borders and title (3); narrow layouts add a step line.
  const height = Math.max(3, rows - 8);
  const body = w.lines().slice(w.scroll, w.scroll + height);
  const main = (
    <Box flexDirection="column" borderStyle="single" flexGrow={1} height={height + 3}>
      <Text bold>{` ${w.title()} `}</Text>
      {body.map((l, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity
        <Text key={i} bold={l.bold ?? false} inverse={l.inverse ?? false}>
          {l.text === "" ? " " : l.text}
        </Text>
      ))}
    </Box>
  );
  return (
    <Box flexDirection="column" width={columns}>
      <Text dimColor wrap="truncate-start">{`agent-harness-tui · ${w.header()}`}</Text>
      {columns >= 80 ? (
        <Box>
          <Box flexDirection="column" borderStyle="single" width={20}>
            {STEPS.map(([, name], i) => (
              <Text key={name} bold={i === current}>
                {`${i === current ? "▸" : " "} ${i + 1} ${name}`}
              </Text>
            ))}
          </Box>
          {main}
        </Box>
      ) : (
        <>
          <Text>{`${current + 1}/${STEPS.length} ${STEPS[current]?.[1]}`}</Text>
          {main}
        </>
      )}
      <Box borderStyle="single">
        <Text>{w.busy ? "처리 중…" : w.keys()}</Text>
      </Box>
    </Box>
  );
}
