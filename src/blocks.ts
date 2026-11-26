// Managed blocks inside user files (Stage 1 spec §3 ownership, §5;
// instruction 0005).
//
// A block is the lines between a begin and an end marker that carry the
// same block ID. Everything outside the markers belongs to the user and is
// kept byte-for-byte. Any malformed, duplicated, nested or mismatched marker
// is a conflict: the file is never rewritten as a whole to "fix" it.

import fs from "node:fs";
import { type Desired, sha256 } from "./apply.js";
import { isSlug } from "./contract.js";
import type { ProjectRoot, RelPath } from "./paths.js";

const TAG = "agent-harness";

/** `markdown`: `<!-- agent-harness:begin ID -->`. `hash`: `# agent-harness:begin ID`. */
export type Syntax = "markdown" | "hash";

function marker(syntax: Syntax, kind: "begin" | "end", id: string): string {
  return syntax === "markdown" ? `<!-- ${TAG}:${kind} ${id} -->` : `# ${TAG}:${kind} ${id}`;
}

/** Parses an exact marker line into [isBegin, id]. */
function parseMarker(syntax: Syntax, line: string): [boolean, string] | undefined {
  let inner: string;
  if (syntax === "markdown") {
    if (!line.startsWith("<!-- ") || !line.endsWith(" -->")) return undefined;
    inner = line.slice(5, -4);
  } else {
    if (!line.startsWith("# ")) return undefined;
    inner = line.slice(2);
  }
  if (!inner.startsWith(`${TAG}:`)) return undefined;
  const rest = inner.slice(TAG.length + 1);
  const sp = rest.indexOf(" ");
  if (sp < 0) return undefined;
  const kind = rest.slice(0, sp);
  const id = rest.slice(sp + 1);
  if (kind !== "begin" && kind !== "end") return undefined;
  return isSlug(id) ? [kind === "begin", id] : undefined;
}

export type BlockErrorKind = "not-utf8" | "invalid-id" | "body-has-marker" | "malformed";

export class BlockError extends Error {
  constructor(
    readonly kind: BlockErrorKind,
    message: string,
    /** `malformed` only: 1-based line number. */
    readonly line?: number,
  ) {
    super(message);
  }
}

const malformed = (line: number, why: string) =>
  new BlockError("malformed", `line ${line}: ${why}`, line);

/** Result of inserting or updating one block. */
export interface BlockEdit {
  /** Full file content after the edit. */
  content: string;
  /** Hash of the block body after the edit (LF-normalized). */
  newHash: string;
  /** Hash of the block body currently in the file, if present. */
  currentHash?: string;
}

/** Splits like Rust `str::lines`: `\n` or `\r\n`, no trailing empty line. */
function lines(text: string): string[] {
  if (text === "") return [];
  const out = text.split("\n");
  if (out.at(-1) === "") out.pop();
  return out.map((l) => (l.endsWith("\r") ? l.slice(0, -1) : l));
}

/** Validates every marker and returns the [begin, end] line span of each block. */
function scan(text: string, syntax: Syntax): Map<string, [number, number]> {
  const blocks = new Map<string, [number, number]>();
  let open: [string, number] | undefined;
  for (const [i, raw] of lines(text).entries()) {
    const line = raw.trim().replace(/^﻿+/, "");
    if (!line.includes(`${TAG}:`)) continue;
    const parsed = parseMarker(syntax, line);
    if (!parsed) throw malformed(i + 1, `unrecognized marker \`${line}\``);
    const [begin, id] = parsed;
    if (begin) {
      if (open) throw malformed(i + 1, `block \`${id}\` starts inside block \`${open[0]}\``);
      if (blocks.has(id)) throw malformed(i + 1, `duplicate block \`${id}\``);
      open = [id, i];
    } else {
      if (!open) throw malformed(i + 1, `end of block \`${id}\` without a begin`);
      if (open[0] !== id) throw malformed(i + 1, `block \`${open[0]}\` is closed by end \`${id}\``);
      blocks.set(id, [open[1], i]);
      open = undefined;
    }
  }
  if (open) throw malformed(open[1] + 1, `block \`${open[0]}\` is never closed`);
  return blocks;
}

const hashBody = (body: string) => sha256(body.replaceAll("\r\n", "\n"));

/**
 * Inserts block `id` with `body` into `existing` (or a new file), or
 * replaces the body of the existing block. Text outside the block is kept.
 * Throws `BlockError`.
 */
export function upsert(
  existing: string | undefined,
  syntax: Syntax,
  id: string,
  body: string,
): BlockEdit {
  if (!isSlug(id)) throw new BlockError("invalid-id", `block id \`${id}\` must be a slug`);
  if (body.includes(`${TAG}:`)) {
    throw new BlockError("body-has-marker", `block body contains \`${TAG}:\` marker text`);
  }
  const text = existing ?? "";
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const bodyLines = lines(body.replaceAll("\r\n", "\n"));
  const newHash = hashBody(bodyLines.map((l) => `${l}\n`).join(""));
  const block =
    [marker(syntax, "begin", id), ...bodyLines, marker(syntax, "end", id)].join(eol) + eol;

  const span = scan(text, syntax).get(id);
  if (!span) {
    // Append, separated from user content by one blank line.
    let content = text;
    if (content !== "") {
      if (!content.endsWith("\n")) content += eol;
      content += eol;
    }
    return { content: content + block, newHash };
  }

  const [start, end] = span;
  // Offsets of each line start; the begin line and the line after the end marker.
  const offsets = [0];
  for (let i = text.indexOf("\n"); i >= 0; i = text.indexOf("\n", i + 1)) offsets.push(i + 1);
  const at = (n: number) => offsets[n] ?? text.length;
  const afterEnd = at(end + 1);
  const currentBody = text.slice(at(start + 1), at(end));
  const hadEol = text.slice(0, afterEnd).endsWith("\n");
  const content =
    text.slice(0, at(start)) +
    (hadEol ? block : block.replace(/[\r\n]+$/, "")) +
    text.slice(afterEnd);
  return { content, newHash, currentHash: hashBody(currentBody) };
}

/**
 * Reads `path` and builds the apply-engine item for block `id`. Problems
 * with the existing file become a planned conflict, never a rewrite.
 */
export function desired(
  root: ProjectRoot,
  path: RelPath,
  syntax: Syntax,
  id: string,
  body: string,
): Desired {
  let bytes: Buffer | undefined;
  try {
    bytes = fs.readFileSync(root.abs(path));
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
  }
  try {
    // ignoreBOM keeps a leading BOM in the text so it is written back.
    const text = bytes && new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
    const edit = upsert(text, syntax, id, body);
    return {
      path,
      content: Buffer.from(edit.content),
      ownership: { kind: "block", id, newHash: edit.newHash, currentHash: edit.currentHash },
    };
  } catch (e) {
    const why = e instanceof BlockError ? e.message : "file is not UTF-8";
    if (!(e instanceof BlockError) && !(e instanceof TypeError)) throw e;
    return {
      path,
      content: Buffer.alloc(0),
      ownership: { kind: "unresolvable", reason: `managed block: ${why}` },
    };
  }
}
