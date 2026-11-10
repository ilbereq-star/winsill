/**
 * Sessions known to the Claude desktop app, from its metadata files (undocumented):
 * <the app's data folder>/claude-code-sessions/<account>/<org>/local_<id>.json.
 * They outlive the CLI process, which the app stops when a session sits idle.
 */
import { existsSync, statSync } from "node:fs";
import { stat } from "node:fs/promises";
import { join } from "node:path";
import { normalizePath } from "./paths.ts";
import { platform } from "./platform/index.ts";
import { isSessionId } from "./protocol-guard.ts";

const SESSIONS = "claude-code-sessions";
const SESSION_FILES = "*/*/local_*.json";

/**
 * The folder the desktop app keeps its data in. On Windows it can be in more than one place:
 * the one in use is the one with sessions, or the one whose sessions changed last if several have.
 */
export function desktopDataDir(candidates = platform.desktopDataDirs()): string {
  if (candidates.length === 1) return candidates[0]!;
  const used = candidates.filter((dir) => existsSync(join(dir, SESSIONS)));
  if (used.length <= 1) return used[0] ?? candidates[0]!;
  return used.map((dir) => ({ dir, at: lastChange(join(dir, SESSIONS)) })).sort((a, b) => b.at - a.at)[0]!.dir;
}

export function desktopSessionsDir() {
  return join(desktopDataDir(), SESSIONS);
}

function lastChange(sessionsDir: string): number {
  let newest = 0;
  for (const path of new Bun.Glob(SESSION_FILES).scanSync({ cwd: sessionsDir, absolute: true })) {
    newest = Math.max(newest, statSync(path, { throwIfNoEntry: false })?.mtimeMs ?? 0);
  }
  return newest;
}

export interface DesktopSession {
  /** The app's id, used in claude:// links. */
  localId: string;
  /** The CLI session id that hooks and `claude agents` report. */
  sessionId: string;
  title: string | null;
  cwd: string;
  lastActivityAt: number;
  /** When the app last put this session in front; 0 if it doesn't say. */
  lastFocusedAt: number;
  archived: boolean;
}

/** Every desktop session, most recently active first. */
export async function listDesktopSessions(dir = desktopSessionsDir()): Promise<DesktopSession[]> {
  const sessions: DesktopSession[] = [];
  const seen = new Set<string>();
  for await (const path of scan(SESSION_FILES, dir)) {
    seen.add(path);
    const session = await read(path);
    if (session) sessions.push(session);
  }
  forgetAllBut(seen, dir);
  return sessions.sort((a, b) => b.lastActivityAt - a.lastActivityAt);
}

export async function findDesktopSession(sessionId: string, dir = desktopSessionsDir()): Promise<DesktopSession | undefined> {
  return (await listDesktopSessions(dir)).find((s) => s.sessionId === sessionId);
}

/** CLI session id of a desktop `local_…` session. */
export async function cliSessionIdFor(localId: string, dir = desktopSessionsDir()): Promise<string | null> {
  for await (const path of scan(`*/*/${localId}.json`, dir)) {
    const session = await read(path);
    if (session) return session.sessionId;
  }
  return null;
}

/** Files matching `pattern` under `dir`. A missing dir (no desktop app) has none. */
async function* scan(pattern: string, dir: string) {
  try {
    yield* new Bun.Glob(pattern).scan({ cwd: dir, absolute: true });
  } catch (error) {
    if ((error as NodeJS.ErrnoException)?.code !== "ENOENT") throw error;
  }
}

/**
 * What each file held when last read. A session's file runs to over 100 KB and the app rewrites
 * only the ones in use, so most reads find the same time and size and skip the parsing.
 */
const cache = new Map<string, { mtimeMs: number; size: number; session: DesktopSession | null }>();

async function read(path: string): Promise<DesktopSession | null> {
  const info = await stat(path).catch(() => null);
  if (!info) return null; // deleted between the listing and now
  const known = cache.get(path);
  if (known && known.mtimeMs === info.mtimeMs && known.size === info.size) return known.session;
  const session = parse(await Bun.file(path).json().catch(() => null));
  cache.set(path, { mtimeMs: info.mtimeMs, size: info.size, session });
  return session;
}

/** Drops what the cache holds for files of `dir` that are no longer there. */
function forgetAllBut(present: Set<string>, dir: string) {
  for (const path of cache.keys()) if (path.startsWith(dir) && !present.has(path)) cache.delete(path);
}

function parse(meta: Record<string, unknown> | null): DesktopSession | null {
  if (!meta || typeof meta.sessionId !== "string" || !meta.sessionId.startsWith("local_")) return null;
  if (!isSessionId(meta.cliSessionId)) return null;
  return {
    localId: meta.sessionId,
    sessionId: meta.cliSessionId,
    title: typeof meta.title === "string" ? meta.title : null,
    cwd: typeof meta.cwd === "string" ? normalizePath(meta.cwd) : "",
    lastActivityAt: typeof meta.lastActivityAt === "number" ? meta.lastActivityAt : 0,
    lastFocusedAt: typeof meta.lastFocusedAt === "number" ? meta.lastFocusedAt : 0,
    archived: meta.isArchived === true,
  };
}
