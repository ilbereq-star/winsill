/**
 * Opens a session in the Claude desktop app.
 * - Known to the app (running or stopped while idle): navigate with the undocumented
 *   `claude://code/continue?session=local_…`; the app restarts the session if needed.
 * - Unknown to the app and closed: `claude --desktop --resume <id>` (documented, CLI ≥ 2.1.285).
 * - Running in a terminal: refused, since resuming it would start a second process on the same conversation.
 */
import { readdir } from "node:fs/promises";
import { join } from "node:path";
import { CLAUDE_LIVE_SESSIONS } from "./config.ts";
import { desktopSessionsDir, findDesktopSession } from "./desktop-sessions.ts";
import { platform } from "./platform/index.ts";
import { isSessionId } from "./protocol-guard.ts";

const LOCAL_ID = /^local_[A-Za-z0-9-]{1,64}$/;

export type OpenResult = { ok: true; command: string[] } | { ok: false; error: string };

interface LiveSession {
  pid: number;
  sessionId: string;
  entrypoint?: string;
}

function isAlive(pid: number) {
  try {
    process.kill(pid, 0); // signal 0 only checks that the process exists
    return true;
  } catch {
    return false;
  }
}

/** A running Claude Code process for this session, from the CLI's internal sessions/<pid>.json in its config folder. */
async function findLive(id: string, dir: string): Promise<LiveSession | undefined> {
  for (const name of await readdir(dir).catch(() => [])) {
    if (!name.endsWith(".json")) continue;
    const info: LiveSession | null = await Bun.file(join(dir, name)).json().catch(() => null);
    if (info?.sessionId === id && isAlive(info.pid)) return info;
  }
}

/** Picks the command that would open the session, without running it. */
export async function planOpen(
  sessionId: string,
  { liveDir = CLAUDE_LIVE_SESSIONS, desktopDir = desktopSessionsDir(), openUrl = platform.openUrl } = {},
): Promise<OpenResult> {
  if (!isSessionId(sessionId)) return { ok: false, error: "invalid session id" };

  const desktop = await findDesktopSession(sessionId, desktopDir);
  if (desktop && LOCAL_ID.test(desktop.localId)) {
    return { ok: true, command: openUrl(`claude://code/continue?session=${desktop.localId}`) };
  }
  const live = await findLive(sessionId, liveDir);
  if (live) {
    return { ok: false, error: `running outside the desktop app (${live.entrypoint}, pid ${live.pid}): open it in its terminal` };
  }
  return { ok: true, command: ["claude", "--desktop", "--resume", sessionId] };
}

export async function openSession(sessionId: string): Promise<OpenResult> {
  const plan = await planOpen(sessionId);
  if (!plan.ok) return plan;

  const proc = Bun.spawn(plan.command, { stdout: "ignore", stderr: "pipe" });
  const code = await proc.exited;
  if (code !== 0) {
    const stderr = (await new Response(proc.stderr).text()).trim().slice(0, 200);
    return { ok: false, error: `${plan.command[0]} exited with ${code}${stderr ? `: ${stderr}` : ""}` };
  }
  return plan;
}
