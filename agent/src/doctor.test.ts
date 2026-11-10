import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { focusSource, report } from "./doctor.ts";
import { findClaude } from "./find-claude.ts";

const DAY = 24 * 60 * 60 * 1000;
const NOW = 100 * DAY;

test("the report lines up one check per line", () => {
  expect(
    report([
      { level: "ok", what: "Agent", detail: "answering" },
      { level: "warn", what: "Claude Code hooks", detail: "not installed" },
      { level: "fail", what: "claude:// links", detail: "no app" },
    ]),
  ).toBe(["  ok    Agent              answering", "  warn  Claude Code hooks  not installed", "  fail  claude:// links    no app"].join("\n"));
});

test("focus comes from the log while it is being written, else from the sessions' files", () => {
  expect(focusSource(NOW - 1000, NOW - 5000, NOW).detail).toBe("from the app's log");
  // The app wasn't used for a week: the log is old, but still the last word.
  expect(focusSource(NOW - 7 * DAY, NOW - 8 * DAY, NOW).detail).toBe("from the app's log");
  // The log stopped weeks ago and the app has been used since.
  expect(focusSource(NOW - 40 * DAY, NOW - 1000, NOW)).toMatchObject({ level: "ok", detail: expect.stringContaining("sessions' files") });
  expect(focusSource(0, NOW - 1000, NOW)).toMatchObject({ level: "ok", detail: expect.stringContaining("sessions' files") });
  expect(focusSource(0, 0, NOW).level).toBe("warn");
});

test("the CLI is found on the PATH, else where its installer puts it", () => {
  const root = mkdtempSync(join(tmpdir(), "find-claude-"));
  const name = process.platform === "win32" ? "claude.exe" : "claude";
  try {
    expect(findClaude({ PATH: root }, root)).toBeNull();

    const installed = join(root, ".local", "bin");
    mkdirSync(installed, { recursive: true });
    writeFileSync(join(installed, name), "", { mode: 0o755 });
    expect(findClaude({ PATH: root }, root)).toBe(join(installed, name));

    writeFileSync(join(root, name), "", { mode: 0o755 });
    expect(findClaude({ PATH: root }, root)).toBe(join(root, name));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
