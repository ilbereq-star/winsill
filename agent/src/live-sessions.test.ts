import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { SessionHub } from "./hub.ts";
import { isAlive, readLiveSessions } from "./live-sessions.ts";
import { startPoller } from "./poller.ts";
import { Store } from "./store.ts";

const A = "0603472f-b7c5-4dae-b7a1-142468264b30";
const B = "e7733503-7ef9-41b1-a4a8-6f88bf173185";

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "live-sessions-"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

/** A file as the CLI writes it, trimmed to a few of its fields. */
function write(pid: number, fields: Record<string, unknown>) {
  writeFileSync(join(dir, `${pid}.json`), JSON.stringify({ pid, cwd: "/tmp/demo", startedAt: 1000, kind: "interactive", version: "2.1.321", ...fields }));
}

const everyoneAlive = () => true;

async function waitFor(condition: () => boolean, timeoutMs = 2000) {
  const start = Date.now();
  while (!condition()) {
    if (Date.now() - start > timeoutMs) throw new Error("timed out");
    await Bun.sleep(5);
  }
}

test("each running process's file becomes a session with its name and status", async () => {
  write(100, { sessionId: A, name: "Panel", status: "busy", entrypoint: "claude-desktop" });
  write(200, { sessionId: B, status: "something new" });

  expect(await readLiveSessions(dir, everyoneAlive)).toEqual([
    { pid: 100, sessionId: A, cwd: "/tmp/demo", name: "Panel", kind: "interactive", status: "busy", startedAt: 1000, entrypoint: "claude-desktop" },
    { pid: 200, sessionId: B, cwd: "/tmp/demo", name: undefined, kind: "interactive", status: undefined, startedAt: 1000, entrypoint: undefined },
  ]);
});

test("a file left behind by a process that is gone is not a running session", async () => {
  write(100, { sessionId: A });
  write(200, { sessionId: B });
  expect((await readLiveSessions(dir, (pid) => pid === 200))!.map((s) => s.sessionId)).toEqual([B]);
  expect(await readLiveSessions(dir, () => false)).toEqual([]);
});

test("isAlive tells this process from a pid nothing has", () => {
  expect(isAlive(process.pid)).toBe(true);
  expect(isAlive(2 ** 22 + 12345)).toBe(false); // above any pid Windows, Linux or macOS hands out
});

test("other files in the folder, and a file that isn't about its own process, are skipped", async () => {
  write(100, { sessionId: A });
  write(200, { sessionId: B, pid: 999 });
  writeFileSync(join(dir, "notes.txt"), "not a session");
  writeFileSync(join(dir, "abc.json"), "{}");
  expect((await readLiveSessions(dir, everyoneAlive))!.map((s) => s.pid)).toEqual([100]);
});

test("no folder, or files in a shape we don't know, means the files can't tell", async () => {
  expect(await readLiveSessions(join(dir, "missing"), everyoneAlive)).toBeNull();
  expect(await readLiveSessions(dir, everyoneAlive)).toEqual([]); // empty folder: nothing is running

  writeFileSync(join(dir, "100.json"), JSON.stringify({ process: 100, session: A })); // a format of the future
  expect(await readLiveSessions(dir, everyoneAlive)).toBeNull();
});

test("the poller follows the files: a status change shows at once, and a file removed ends its session", async () => {
  const hub = new SessionHub(new Store());
  write(process.pid, { sessionId: A, name: "Panel", status: "idle" });
  const stop = startPoller(hub, { dir, intervalMs: 60_000, askCli: () => Promise.reject(new Error("the CLI must not be asked")) });
  try {
    await waitFor(() => hub.get(A)?.state === "idle");
    expect(hub.get(A)).toMatchObject({ name: "Panel", live: true });

    write(process.pid, { sessionId: A, name: "Panel", status: "busy" }); // no timer in sight: only the change notification
    await waitFor(() => hub.get(A)?.state === "working");

    rmSync(join(dir, `${process.pid}.json`));
    await waitFor(() => hub.get(A)?.live === false);
    expect(hub.get(A)!.state).toBe("ended");
  } finally {
    stop();
  }
});

test("without the files the poller asks the CLI, no more often than told", async () => {
  const hub = new SessionHub(new Store());
  let asked = 0;
  const askCli = async () => {
    asked++;
    return [{ sessionId: A, name: "Panel", cwd: "/tmp/demo", status: "busy" as const, startedAt: 0 }];
  };
  const stop = startPoller(hub, { dir: join(dir, "missing"), intervalMs: 10, cliIntervalMs: 60_000, askCli });
  try {
    await waitFor(() => hub.get(A)?.state === "working");
    await Bun.sleep(60); // several intervals
    expect(asked).toBe(1);
  } finally {
    stop();
  }
});
