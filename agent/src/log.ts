/**
 * Where the agent's lines go: the terminal, until `log.toFile` points them at a file. The file is
 * for when nothing collects the agent's output, as on Windows; it is kept small by rotation.
 */
import { appendFileSync, mkdirSync, renameSync, rmSync, statSync } from "node:fs";
import { dirname } from "node:path";

const MAX_BYTES = 1024 * 1024;
/** agent.log plus two older ones: 3 MB at most. */
const KEPT = 3;

let file: string | null = null;
let size = 0;

function write(line: string, toConsole: (line: string) => void) {
  const now = new Date();
  if (!file) return toConsole(`${now.toLocaleTimeString("en-GB", { hour12: false })}  ${line}`);
  const text = `${now.toISOString()}  ${line}\n`;
  const bytes = Buffer.byteLength(text);
  try {
    if (size + bytes > MAX_BYTES) rotate(file);
    appendFileSync(file, text);
    size += bytes;
  } catch {
    // A log that can't be written must not take the agent down.
  }
}

/** agent.log → agent.log.1 → agent.log.2; the oldest is dropped. */
function rotate(path: string) {
  rmSync(`${path}.${KEPT - 1}`, { force: true });
  for (let n = KEPT - 2; n >= 1; n--) renameIfThere(`${path}.${n}`, `${path}.${n + 1}`);
  renameIfThere(path, `${path}.1`);
  size = 0;
}

function renameIfThere(from: string, to: string) {
  try {
    renameSync(from, to);
  } catch (error) {
    if ((error as NodeJS.ErrnoException)?.code !== "ENOENT") throw error;
  }
}

export const log = {
  info: (line: string) => write(line, console.log),
  error: (line: string) => write(line, console.error),
  /** From now on lines are appended to `path`, each with its time. */
  toFile(path: string) {
    mkdirSync(dirname(path), { recursive: true });
    file = path;
    size = statSync(path, { throwIfNoEntry: false })?.size ?? 0;
  },
  toConsole() {
    file = null;
  },
};
