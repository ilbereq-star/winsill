/**
 * The Claude Code processes that are running, from the file each one keeps in the CLI's config
 * folder: sessions/<pid>.json, rewritten when its status changes. `claude agents --json` lists
 * the same; reading the files costs next to nothing, where the command starts a process.
 * The files are internal to the CLI, so the command remains the fallback (poller.ts).
 */
import { readdir } from "node:fs/promises";
import { join } from "node:path";
import { CLAUDE_LIVE_SESSIONS } from "./config.ts";
import { isSessionId } from "./protocol-guard.ts";
import type { AgentEntry } from "./state.ts";

export interface LiveSession extends AgentEntry {
  pid: number;
  /** What started the process: "cli", "claude-desktop"… */
  entrypoint?: string;
}

const STATUSES = ["busy", "waiting", "idle"] as const;
const PID_FILE = /^(\d+)\.json$/;

export function isAlive(pid: number) {
  try {
    process.kill(pid, 0); // signal 0 only checks that the process exists
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException)?.code === "EPERM"; // it exists, but belongs to someone else
  }
}

/**
 * Every running session, or null when the files can't tell: the folder is missing (a CLI that
 * doesn't write them), or there are files and none reads as a session (their format changed).
 */
export async function readLiveSessions(dir = CLAUDE_LIVE_SESSIONS, alive = isAlive): Promise<LiveSession[] | null> {
  let names: string[];
  try {
    names = await readdir(dir);
  } catch {
    return null;
  }
  const sessions: LiveSession[] = [];
  let unknown = 0;
  for (const name of names) {
    const pid = Number(name.match(PID_FILE)?.[1]);
    if (!pid) continue;
    const session = parse(await readJson(join(dir, name)), pid);
    if (!session) unknown++;
    else if (alive(pid)) sessions.push(session);
  }
  return unknown > 0 && sessions.length === 0 ? null : sessions;
}

/** A file caught while the CLI rewrites it reads as nothing: give it a moment before giving up on it. */
async function readJson(path: string): Promise<unknown> {
  for (let attempt = 1; ; attempt++) {
    const value = await Bun.file(path).json().catch(() => null);
    if (value !== null || attempt === 3) return value;
    await Bun.sleep(20);
  }
}

/** Null unless the file describes the process it is named after. */
function parse(file: unknown, pid: number): LiveSession | null {
  const f = file as Record<string, unknown> | null;
  if (typeof f !== "object" || f === null || f.pid !== pid || !isSessionId(f.sessionId) || typeof f.cwd !== "string") return null;
  return {
    pid,
    sessionId: f.sessionId,
    cwd: f.cwd,
    name: typeof f.name === "string" ? f.name : undefined,
    kind: typeof f.kind === "string" ? f.kind : undefined,
    status: STATUSES.find((status) => status === f.status),
    startedAt: typeof f.startedAt === "number" ? f.startedAt : 0,
    entrypoint: typeof f.entrypoint === "string" ? f.entrypoint : undefined,
  };
}
