import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { appendFileSync, mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { lastFocus, watchDesktopFocus, watchLastFocused } from "./desktop-focus.ts";
import { cliSessionIdFor, desktopDataDir, findDesktopSession, listDesktopSessions } from "./desktop-sessions.ts";

const LOCAL_A = "local_52223c18-ba45-4a92-9290-45eb61d71d5d";
const LOCAL_B = "local_e810f230-1637-4569-a5aa-7021561c21b3";
const CLI_A = "0603472f-b7c5-4dae-b7a1-142468264b30";
const CLI_B = "e7733503-7ef9-41b1-a4a8-6f88bf173185";
const focusLine = (id: string) => `2026-11-06 21:12:08 [info] [CCD] LocalSessions.setFocusedSession: sessionId=${id}\n`;

let dir: string;
let log: string;
let sessionsDir: string;

beforeEach(() => {
  dir = mkdtempSync(`${tmpdir()}/desktop-focus-`);
  log = `${dir}/main.log`;
  sessionsDir = `${dir}/sessions`;
  mkdirSync(`${sessionsDir}/account/org`, { recursive: true });
  const sessions = [
    { sessionId: LOCAL_A, cliSessionId: CLI_A, title: "Panel", cwd: "/Users/dev/panel", lastActivityAt: 2000 },
    { sessionId: LOCAL_B, cliSessionId: CLI_B, title: "Infra", cwd: "/Users/dev/infra", lastActivityAt: 3000, isArchived: true },
  ];
  for (const s of sessions) writeFileSync(`${sessionsDir}/account/org/${s.sessionId}.json`, JSON.stringify(s));
  writeFileSync(`${sessionsDir}/account/org/local_broken.json`, "{ not json");
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

async function waitFor(condition: () => boolean, timeoutMs = 3000) {
  const start = Date.now();
  while (!condition()) {
    if (Date.now() - start > timeoutMs) throw new Error("timed out");
    await Bun.sleep(10);
  }
}

describe("lastFocus", () => {
  test("returns the last focus change, null included", () => {
    expect(lastFocus(focusLine("null") + focusLine(LOCAL_A))).toBe(LOCAL_A);
    expect(lastFocus(focusLine(LOCAL_A) + focusLine("null"))).toBeNull();
    expect(lastFocus("2026-11-06 [info] something else\n")).toBeUndefined();
  });
});

describe("desktop sessions", () => {
  test("cliSessionIdFor maps desktop ids through the app's session metadata", async () => {
    expect(await cliSessionIdFor(LOCAL_A, sessionsDir)).toBe(CLI_A);
    expect(await cliSessionIdFor("local_unknown", sessionsDir)).toBeNull();
  });

  test("listDesktopSessions reads every valid file, most recent first", async () => {
    const sessions = await listDesktopSessions(sessionsDir);
    expect(sessions.map((s) => [s.localId, s.sessionId, s.archived])).toEqual([
      [LOCAL_B, CLI_B, true],
      [LOCAL_A, CLI_A, false],
    ]);
    expect(await findDesktopSession(CLI_A, sessionsDir)).toMatchObject({ title: "Panel", cwd: "/Users/dev/panel" });
  });

  test("a missing sessions dir, as without the desktop app, means no desktop sessions", async () => {
    const missing = `${dir}/missing`;
    expect(await listDesktopSessions(missing)).toEqual([]);
    expect(await findDesktopSession(CLI_A, missing)).toBeUndefined();
    expect(await cliSessionIdFor(LOCAL_A, missing)).toBeNull();
  });

  test("any other failure to read the sessions dir still throws", async () => {
    writeFileSync(log, ""); // a file, so scanning it as a dir fails with ENOTDIR
    await expect(listDesktopSessions(log)).rejects.toThrow("ENOTDIR");
    await expect(cliSessionIdFor(LOCAL_A, log)).rejects.toThrow("ENOTDIR");
  });
});

describe("desktopDataDir, when the app's data may be in more than one folder", () => {
  /** A data folder with one session file last written at `mtime` (epoch seconds), or with no sessions at all. */
  function dataDir(name: string, mtime?: number) {
    const root = join(dir, name);
    mkdirSync(root, { recursive: true });
    if (mtime !== undefined) {
      mkdirSync(join(root, "claude-code-sessions", "account", "org"), { recursive: true });
      const file = join(root, "claude-code-sessions", "account", "org", `${LOCAL_A}.json`);
      writeFileSync(file, "{}");
      utimesSync(file, mtime, mtime);
    }
    return root;
  }

  test("the only folder with sessions wins, wherever it is in the list", () => {
    const [empty, used] = [dataDir("empty"), dataDir("used", 1000)];
    expect(desktopDataDir([empty, used])).toBe(used);
    expect(desktopDataDir([used, join(dir, "not-there")])).toBe(used);
  });

  test("when several have sessions, the one written last wins", () => {
    const [stale, live] = [dataDir("stale", 1000), dataDir("live", 2000)];
    expect(desktopDataDir([stale, live])).toBe(live);
    expect(desktopDataDir([live, stale])).toBe(live);
  });

  test("with no sessions anywhere, as before the app's first run, it is the usual folder", () => {
    expect(desktopDataDir([join(dir, "usual"), join(dir, "other")])).toBe(join(dir, "usual"));
    expect(desktopDataDir([join(dir, "single")])).toBe(join(dir, "single"));
  });
});

test("watchDesktopFocus follows switches, skips the transient null and survives rotation", async () => {
  writeFileSync(log, focusLine(LOCAL_A));
  const seen: (string | null)[] = [];
  const stop = watchDesktopFocus((id) => seen.push(id), { log, sessionsDir, pollMs: 20 });
  try {
    await waitFor(() => seen.length === 1);
    appendFileSync(log, focusLine("null") + focusLine(LOCAL_B));
    await waitFor(() => seen.length === 2);
    writeFileSync(log, focusLine(LOCAL_A)); // rotated: a fresh, shorter file
    await waitFor(() => seen.length === 3);
    expect(seen).toEqual([CLI_A, CLI_B, CLI_A]);
  } finally {
    stop();
  }
});

test("a null that lasts means nothing is in front", async () => {
  writeFileSync(log, focusLine(LOCAL_A));
  const seen: (string | null)[] = [];
  const stop = watchDesktopFocus((id) => seen.push(id), { log, sessionsDir, pollMs: 20 });
  try {
    await waitFor(() => seen.length === 1);
    appendFileSync(log, focusLine("null"));
    await waitFor(() => seen.length === 2);
    expect(seen).toEqual([CLI_A, null]);
  } finally {
    stop();
  }
});

describe("watchLastFocused, for when the log says nothing", () => {
  const focusedAt = (local: string, cli: string, at?: number) =>
    writeFileSync(`${sessionsDir}/account/org/${local}.json`, JSON.stringify({ sessionId: local, cliSessionId: cli, lastFocusedAt: at }));

  test("reports the session focused last, and again when another one takes over", async () => {
    focusedAt(LOCAL_A, CLI_A, 5000);
    focusedAt(LOCAL_B, CLI_B, 4000);
    const seen: [string | null, number][] = [];
    const stop = watchLastFocused((id, at) => seen.push([id, at]), { sessionsDir, pollMs: 60_000 }); // no timer in sight: only change notifications
    try {
      await waitFor(() => seen.length === 1);
      focusedAt(LOCAL_B, CLI_B, 6000);
      await waitFor(() => seen.length === 2);
      expect(seen).toEqual([
        [CLI_A, 5000],
        [CLI_B, 6000],
      ]);
    } finally {
      stop();
    }
  });

  test("a session written again while it stays in front is not reported twice", async () => {
    focusedAt(LOCAL_A, CLI_A, 5000);
    const seen: (string | null)[] = [];
    const stop = watchLastFocused((id) => seen.push(id), { sessionsDir, pollMs: 20 });
    try {
      await waitFor(() => seen.length === 1);
      focusedAt(LOCAL_A, CLI_A, 5500); // the app rewrites the file on every activity
      await Bun.sleep(150);
      expect(seen).toEqual([CLI_A]);
    } finally {
      stop();
    }
  });

  test("sessions that don't say when they were focused report nothing", async () => {
    const seen: (string | null)[] = [];
    const stop = watchLastFocused((id) => seen.push(id), { sessionsDir, pollMs: 20 });
    await Bun.sleep(150);
    stop();
    expect(seen).toEqual([]);
  });
});

test("a session file that changes or goes away is read again, not remembered", async () => {
  const file = `${sessionsDir}/account/org/${LOCAL_A}.json`;
  expect((await findDesktopSession(CLI_A, sessionsDir))!.title).toBe("Panel");

  writeFileSync(file, JSON.stringify({ sessionId: LOCAL_A, cliSessionId: CLI_A, title: "Renamed panel" }));
  expect((await findDesktopSession(CLI_A, sessionsDir))!.title).toBe("Renamed panel");
  expect(await cliSessionIdFor(LOCAL_A, sessionsDir)).toBe(CLI_A);

  rmSync(file);
  expect(await findDesktopSession(CLI_A, sessionsDir)).toBeUndefined();
  expect(await cliSessionIdFor(LOCAL_A, sessionsDir)).toBeNull();
});
