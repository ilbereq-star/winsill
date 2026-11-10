/**
 * `doctor`: looks at everything the agent depends on and says what it found. Half of it is
 * undocumented and can change with any Claude update; this is where that shows.
 */
import { existsSync } from "node:fs";
import { stat } from "node:fs/promises";
import { PANEL_PORT } from "../../shared/protocol.ts";
import { CLAUDE_LIVE_SESSIONS, CLAUDE_SETTINGS, CONFIG_FILE } from "./config.ts";
import { desktopDataDir, desktopSessionsDir, listDesktopSessions } from "./desktop-sessions.ts";
import { findClaude } from "./find-claude.ts";
import { readLiveSessions } from "./live-sessions.ts";
import { platform } from "./platform/index.ts";
import { agentHealth, hooksStatus } from "./status.ts";

export interface Check {
  /** "fail": something the strip is for can't work. "warn": it works, with less. */
  level: "ok" | "warn" | "fail";
  what: string;
  detail: string;
}

const DAY_MS = 24 * 60 * 60 * 1000;

export async function doctor(version: string) {
  const checks = await runChecks();
  console.log(`${platform.app} ${version}\n${report(checks)}`);
  if (checks.some((check) => check.level === "fail")) process.exitCode = 1;
}

export function report(checks: Check[]): string {
  const width = Math.max(...checks.map((check) => check.what.length));
  return checks.map((check) => `  ${check.level.padEnd(4)}  ${check.what.padEnd(width)}  ${check.detail}`).join("\n");
}

async function runChecks(): Promise<Check[]> {
  return [await cli(), await liveSessions(), await hooks(), ...(await desktopApp()), ...(await linkHandler()), await agent()];
}

async function cli(): Promise<Check> {
  const what = "Claude Code CLI";
  const path = findClaude();
  if (!path) return { level: "warn", what, detail: "not found on the PATH nor in ~/.local/bin: a session the desktop app doesn't know can't be opened" };
  const claude = Bun.spawn([path, "--version"], { stdout: "pipe", stderr: "ignore", windowsHide: true });
  const version = (await new Response(claude.stdout).text()).trim();
  return { level: "ok", what, detail: `${path}${version ? ` (${version})` : ""}` };
}

async function liveSessions(): Promise<Check> {
  const what = "Running sessions";
  const live = await readLiveSessions();
  if (live) return { level: "ok", what, detail: `${live.length} in ${CLAUDE_LIVE_SESSIONS}` };
  return { level: "warn", what, detail: `${CLAUDE_LIVE_SESSIONS} is missing or not as expected: the agent asks \`claude agents\` instead, which is slower` };
}

async function hooks(): Promise<Check> {
  const what = "Claude Code hooks";
  switch (await hooksStatus()) {
    case "current":
      return { level: "ok", what, detail: `installed in ${CLAUDE_SETTINGS}` };
    case "outdated":
      return { level: "warn", what, detail: `in ${CLAUDE_SETTINGS}, but not as this version writes them: run \`${platform.app} setup\` again` };
    case "missing":
      return { level: "warn", what, detail: `not installed: states arrive late and without detail. Run \`${platform.app} setup\`` };
  }
}

async function desktopApp(): Promise<Check[]> {
  const dir = desktopDataDir();
  if (!existsSync(desktopSessionsDir())) {
    const looked = platform.desktopDataDirs().join(", ");
    return [{ level: "warn", what: "Claude desktop app", detail: `no sessions found (looked in ${looked}): nothing to pin or open` }];
  }
  const sessions = await listDesktopSessions();
  const lastFocused = Math.max(0, ...sessions.map((s) => s.lastFocusedAt));
  const logWritten = (await stat(platform.desktopLog(dir)).catch(() => null))?.mtimeMs ?? 0;
  return [
    { level: "ok", what: "Claude desktop app", detail: `${sessions.length} sessions in ${dir}` },
    focusSource(logWritten, lastFocused, Date.now()),
  ];
}

/** Which of the two focus sources is alive, going by when each last said something. */
export function focusSource(logWritten: number, lastFocused: number, now: number): Check {
  const what = "Session in front";
  const fresh = (at: number) => at > 0 && now - at < DAY_MS;
  if (fresh(logWritten) || (logWritten > 0 && logWritten >= lastFocused)) return { level: "ok", what, detail: "from the app's log" };
  if (lastFocused > 0) return { level: "ok", what, detail: "from the sessions' files: the app's log is silent, so losing the focus goes unnoticed" };
  return { level: "warn", what, detail: "unknown: the app's log is silent and its sessions don't say when they were focused" };
}

async function linkHandler(): Promise<Check[]> {
  const what = "claude:// links";
  const handler = await platform.urlHandler("claude");
  if (handler === undefined) return []; // can't be told here
  if (handler === null) return [{ level: "fail", what, detail: "no app is registered for them: a session can't be opened in the desktop app" }];
  return [{ level: "ok", what, detail: `opened by ${handler}` }];
}

async function agent(): Promise<Check> {
  const what = "Agent";
  const health = await agentHealth();
  if (health) return { level: "ok", what, detail: `answering on :${PANEL_PORT}, ${health.sessions} sessions` };
  const first = existsSync(CONFIG_FILE) ? "" : " (it has never run here)";
  return { level: "warn", what, detail: `not running${first}: start it with \`${platform.app} agent\`` };
}
