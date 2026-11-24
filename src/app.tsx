import { Box, Text, useApp, useInput } from "ink";

/** Top-level screen. The Stage 1 flow is ported here in U6. */
export function App({ root }: { root: string }) {
  const { exit } = useApp();
  useInput((input, key) => {
    if (input === "q" || key.escape) exit();
  });
  return (
    <Box flexDirection="column">
      <Text bold>agent-harness-tui</Text>
      <Text>프로젝트 루트: {root}</Text>
      <Text dimColor>q 또는 Esc: 종료</Text>
    </Box>
  );
}
