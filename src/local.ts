// `.harness/local.toml`: machine-local values (Stage 2 spec §2,
// `HARNESS_CATALOG_CONTRACT`). `.harness/` is ignored by Git, so the
// optional gateway token stored here is never tracked.

import fs from "node:fs";
import path from "node:path";
import { inspect } from "node:util";
import { parse, stringify } from "smol-toml";

export const LOCAL_FILE = ".harness/local.toml";

/** A secret that never shows up in `inspect`, `JSON.stringify` or string conversion. */
export class Secret {
  readonly #value: string;

  private constructor(value: string) {
    this.#value = value;
  }

  /** Undefined for an empty or blank value. */
  static from(value: string): Secret | undefined {
    const v = value.trim();
    return v === "" ? undefined : new Secret(v);
  }

  expose(): string {
    return this.#value;
  }

  toString(): string {
    return "Secret(<redacted>)";
  }

  toJSON(): string {
    return this.toString();
  }

  [inspect.custom](): string {
    return this.toString();
  }
}

const file = (root: string) => path.join(root, LOCAL_FILE);

/**
 * The stored gateway token, if any. Throws an `Error` whose message never
 * quotes the file, because parser messages could echo the token.
 */
export function loadToken(root: string): Secret | undefined {
  let text: string;
  try {
    text = fs.readFileSync(file(root), "utf8");
  } catch (e) {
    const err = e as NodeJS.ErrnoException;
    if (err.code === "ENOENT") return undefined;
    throw new Error(`${LOCAL_FILE}: ${err.code ?? "read failed"}`);
  }
  const invalid = () =>
    new Error(`${LOCAL_FILE} is invalid (unknown key or wrong value); the token is not used`);
  let doc: Record<string, unknown>;
  try {
    doc = parse(text, { unsafeKeyBehaviour: "throw" });
  } catch {
    throw invalid();
  }
  const { schema_version: version, gateway, ...rest } = doc;
  if (typeof version !== "number" || !Number.isInteger(version)) throw invalid();
  if (version !== 1) throw new Error(`${LOCAL_FILE}: unsupported schema_version ${version}`);
  if (Object.keys(rest).length > 0) throw invalid();
  if (gateway === undefined) return undefined;
  if (typeof gateway !== "object" || gateway === null || Array.isArray(gateway)) throw invalid();
  const { token, ...extra } = gateway as Record<string, unknown>;
  if (Object.keys(extra).length > 0 || (token !== undefined && typeof token !== "string")) {
    throw invalid();
  }
  return token === undefined ? undefined : Secret.from(token);
}

/** Writes the token, replacing the file. Owner-only permissions on Unix. */
export function saveToken(root: string, token: Secret): void {
  const text = stringify({ schema_version: 1, gateway: { token: token.expose() } });
  const target = file(root);
  const dir = path.dirname(target);
  const tmp = path.join(dir, "local.toml.tmp");
  try {
    fs.mkdirSync(dir, { recursive: true });
    const fd = fs.openSync(tmp, "w", 0o600);
    try {
      // `mode` above only applies when the file is created.
      if (process.platform !== "win32") fs.fchmodSync(fd, 0o600);
      fs.writeFileSync(fd, text);
      fs.fsyncSync(fd);
    } finally {
      fs.closeSync(fd);
    }
    fs.renameSync(tmp, target);
  } catch (e) {
    fs.rmSync(tmp, { force: true });
    throw new Error(`${LOCAL_FILE}: ${(e as NodeJS.ErrnoException).code ?? "write failed"}`);
  }
}
