import { afterEach, describe, expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { SessionHub, type HubChange } from "./hub.ts";
import { parseClientMessage } from "./protocol-guard.ts";
import type { HookEvent } from "./state.ts";
import { Store } from "./store.ts";

const A = "0603472f-b7c5-4dae-b7a1-142468264b30";
const B = "34446bd8-9f71-4831-9f0b-d6094803dee2";
const ev = (session_id: string, hook_event_name: string, extra: Record<string, unknown> = {}): HookEvent => ({
  session_id,
  hook_event_name,
  cwd: "/Users/dev/claude-panel",
  ...extra,
});

function setup(store = new Store(), start = 1_000_000) {
  let now = start;
  const hub = new SessionHub(store, () => now);
  const changes: HubChange[] = [];
  hub.subscribe((c) => changes.push(c));
  return { hub, store, changes, tick: (ms: number) => (now += ms) };
}

describe("SessionHub", () => {
  test("a hook event creates a session named after its folder until the poll names it", () => {
    const { hub } = setup();
    hub.handleHook(ev(A, "UserPromptSubmit"));
    expect(hub.get(A)).toMatchObject({ name: "claude-panel", state: "working", live: true, pinned: false });

    hub.handleAgents([{ sessionId: A, name: "Claude panel", cwd: "/Users/dev/claude-panel", status: "busy", startedAt: 0 }]);
    expect(hub.get(A)!.name).toBe("Claude panel");
  });

  test("only visible changes are announced", () => {
    const { hub, changes } = setup();
    const tool = { tool_name: "Bash", tool_input: { command: "ls" } };
    hub.handleHook(ev(A, "PreToolUse", tool));
    hub.handleHook(ev(A, "PostToolUse", tool)); // same state, same activity
    hub.handleHook(ev(A, "Stop"));
    expect(changes.map((c) => c.type === "session" && c.session.state)).toEqual(["working", "done"]);
  });

  test("the store is written only when a session changes in a way worth keeping", () => {
    const store = new Store();
    let saves = 0;
    const save = store.saveSession.bind(store);
    store.saveSession = (session) => {
      saves++;
      save(session);
    };
    const { hub } = setup(store);
    const tool = { tool_name: "Bash", tool_input: { command: "ls" } };

    hub.handleHook(ev(A, "PreToolUse", tool));
    for (let i = 0; i < 20; i++) hub.handleHook(ev(A, "PostToolUse", tool)); // a burst that only moves lastEventAt
    expect(saves).toBe(1);

    hub.handleAgents([{ sessionId: A, cwd: "/Users/dev/claude-panel", pid: 42, status: "busy", startedAt: 0 }]); // the pid is kept too
    hub.handleHook(ev(A, "Stop"));
    expect(saves).toBe(3);
    expect(store.loadSessions()).toMatchObject([{ id: A, state: "done", pid: 42 }]);
  });

  test("pins get consecutive positions; unpinning resets the icon", () => {
    const { hub } = setup();
    hub.handleHook(ev(A, "Stop"));
    hub.handleHook(ev(B, "Stop"));

    hub.setPinned(A, true);
    hub.setPinned(B, true);
    expect(hub.get(A)).toMatchObject({ pinned: true, position: 0 });
    expect(hub.get(B)).toMatchObject({ pinned: true, position: 1 });

    hub.setIcon(A, { character: "crab", color: "#ff6a1a" });
    hub.setPinned(A, false);
    expect(hub.get(A)).toMatchObject({ pinned: false, position: null, icon: { character: "default" } });
  });

  test("setting an icon pins the session if needed and keeps an existing position", () => {
    const { hub } = setup();
    hub.handleHook(ev(A, "Stop"));
    hub.handleHook(ev(B, "Stop"));
    hub.setPinned(A, true);

    expect(hub.setIcon(B, { character: "crab", color: "#ff6a1a" })).toBe(true);
    expect(hub.get(B)).toMatchObject({ pinned: true, position: 1, icon: { character: "crab", color: "#ff6a1a" } });

    hub.setIcon(A, { character: "ghost", color: "#00ff00" });
    expect(hub.get(A)).toMatchObject({ pinned: true, position: 0, icon: { character: "ghost" } });
  });

  test("adopting a session that isn't running tracks it as closed, and the poll revives it", () => {
    const { hub } = setup();
    hub.adopt(A, "Idle work", "/Users/dev/idle", 5000);
    hub.adopt(A, "ignored", "/ignored", 0);
    expect(hub.get(A)).toMatchObject({ name: "Idle work", live: false, state: "ended", stateSince: 5000 });

    hub.handleAgents([{ sessionId: A, name: "Idle work", cwd: "/Users/dev/idle", status: "idle", startedAt: 0 }]);
    expect(hub.get(A)).toMatchObject({ live: true, state: "idle" });
  });

  test("focus is announced once per change", () => {
    const { hub, changes } = setup();
    hub.setFocused(A);
    hub.setFocused(A);
    hub.setFocused(null);
    expect(changes).toEqual([
      { type: "focus", id: A },
      { type: "focus", id: null },
    ]);
  });

  test("unknown sessions cannot be pinned or given an icon", () => {
    const { hub } = setup();
    expect(hub.setPinned(A, true)).toBe(false);
    expect(hub.setIcon(A, { character: "crab", color: "#ff6a1a" })).toBe(false);
  });

  describe("with a database file", () => {
    const path = `${tmpdir()}/claude-panel-test-${process.pid}.db`;
    afterEach(() => {
      for (const suffix of ["", "-wal", "-shm"]) rmSync(path + suffix, { force: true });
    });

    test("sessions and pins survive a restart", () => {
      const first = setup(new Store(path));
      first.hub.handleHook(ev(A, "Stop"));
      first.hub.setPinned(A, true);
      first.hub.setIcon(A, { character: "ghost", color: "#00ff00" });
      first.store.close();

      const second = setup(new Store(path));
      expect(second.hub.get(A)).toMatchObject({ state: "done", pinned: true, icon: { character: "ghost" } });
      second.store.close();
    });
  });

  test("prune forgets old ended sessions unless pinned", () => {
    const { hub, changes, tick } = setup();
    hub.handleHook(ev(A, "SessionEnd", { reason: "other" }));
    hub.handleHook(ev(B, "SessionEnd", { reason: "other" }));
    hub.setPinned(B, true);
    tick(25 * 60 * 60 * 1000);

    hub.prune();
    expect(hub.has(A)).toBe(false);
    expect(hub.has(B)).toBe(true);
    expect(changes.at(-1)).toEqual({ type: "removed", id: A });
  });
});

describe("parseClientMessage", () => {
  test("accepts the whitelisted commands", () => {
    expect(parseClientMessage(JSON.stringify({ type: "library" }))).toEqual({ type: "library" });
    expect(parseClientMessage(JSON.stringify({ type: "open", id: A }))).toEqual({ type: "open", id: A });
    expect(parseClientMessage(JSON.stringify({ type: "pin", id: A, pinned: true }))).toEqual({ type: "pin", id: A, pinned: true });
    expect(parseClientMessage(JSON.stringify({ type: "icon", id: A, icon: { character: "crab", color: "#FF6A1A" } }))).toEqual({
      type: "icon",
      id: A,
      icon: { character: "crab", color: "#ff6a1a" },
    });
  });

  test("rejects everything else", () => {
    const bad = [
      "not json",
      "null",
      JSON.stringify({ type: "open", id: "; rm -rf /" }),
      JSON.stringify({ type: "exec", id: A, command: "ls" }),
      JSON.stringify({ type: "pin", id: A, pinned: "yes" }),
      JSON.stringify({ type: "icon", id: A, icon: { character: "../etc", color: "#fff" } }),
    ];
    for (const raw of bad) expect(parseClientMessage(raw)).toBeNull();
  });
});
