import { describe, expect, test } from "bun:test";
import { buildPanelHooks, hasPanelHooks, HOOK_URL, hooksState, mergeHooks, PANEL_EVENTS } from "./hooks-config.ts";

const theirs = { type: "command", command: "echo mine" };
const ours = buildPanelHooks("secret", "command");

describe("buildPanelHooks", () => {
  test("an http hook carries the token in a header and gives up after two seconds", () => {
    const hooks = buildPanelHooks("secret", "http");
    expect(hooks.Stop).toEqual([{ hooks: [{ type: "http", url: HOOK_URL, headers: { Authorization: "Bearer secret" }, timeout: 2 }] }]);
  });

  test("every event gets a hook, but SessionStart over http, where Claude Code would skip it", () => {
    expect(Object.keys(ours)).toEqual([...PANEL_EVENTS]);
    expect(Object.keys(buildPanelHooks("secret", "http"))).toEqual(PANEL_EVENTS.filter((event) => event !== "SessionStart"));
  });

  test("a command hook sends the same token with curl, and never fails", () => {
    const [handler] = ours.Stop![0]!.hooks;
    expect(handler!.type).toBe("command");
    expect(handler!.command).toContain("-H 'Authorization: Bearer secret'");
    expect(handler!.command).toContain(HOOK_URL);
    expect(handler!.command).toEndWith("|| true");
  });

  test("no hook on PermissionRequest, where a hook can answer the prompt for the user", () => {
    expect(Object.keys(buildPanelHooks("secret", "http"))).not.toContain("PermissionRequest");
  });
});

describe("mergeHooks", () => {
  test("install adds every event and keeps the user's hooks and other settings", () => {
    const merged = mergeHooks({ theme: "dark", hooks: { Stop: [{ hooks: [theirs] }] } }, ours);
    expect(merged.theme).toBe("dark");
    expect(merged.hooks?.Stop).toEqual([{ hooks: [theirs] }, ...ours.Stop!]);
    expect(Object.keys(merged.hooks!).sort()).toEqual(Object.keys(ours).sort());
  });

  test("installing again changes nothing", () => {
    const once = mergeHooks({}, ours);
    expect(mergeHooks(once, ours)).toEqual(once);
  });

  test("replaces our earlier hooks, in any style or with an old token, instead of duplicating them", () => {
    const old = { hooks: { Stop: [{ hooks: [{ type: "http", url: HOOK_URL }] }], SessionStart: buildPanelHooks("old-token", "command").SessionStart! } };
    expect(mergeHooks(old, ours)).toEqual(mergeHooks({}, ours));
  });

  test("remove drops only ours, then empty groups, events and the hooks key", () => {
    const installed = mergeHooks({ hooks: { Stop: [{ hooks: [theirs] }] } }, ours);
    expect(mergeHooks(installed, null)).toEqual({ hooks: { Stop: [{ hooks: [theirs] }] } });
    expect(mergeHooks(mergeHooks({ model: "opus" }, ours), null)).toEqual({ model: "opus" });
  });
});

test("hasPanelHooks", () => {
  expect(hasPanelHooks({})).toBe(false);
  expect(hasPanelHooks({ hooks: { Stop: [{ hooks: [theirs] }] } })).toBe(false);
  expect(hasPanelHooks(mergeHooks({}, ours))).toBe(true);
});

test("hooksState tells hooks that need installing again from ones that are fine", () => {
  expect(hooksState({ hooks: { Stop: [{ hooks: [theirs] }] } }, ours)).toBe("missing");
  expect(hooksState(mergeHooks({ model: "opus" }, ours), ours)).toBe("current");
  expect(hooksState(mergeHooks({}, buildPanelHooks("old-token", "command")), ours)).toBe("outdated");
  expect(hooksState(mergeHooks({}, buildPanelHooks("secret", "http")), ours)).toBe("outdated");
});
