/**
 * Which session is in front in the Claude desktop app, from two undocumented sources:
 * - its log, tailed for `LocalSessions.setFocusedSession` lines: exact, and the only one that
 *   says when no session is in front;
 * - the `lastFocusedAt` in each session's file: for when the log says nothing, as on Windows,
 *   where the app has been seen to stop writing it.
 * Each reports a time with the session, so whoever listens can keep the latest word of the two.
 * If either source changes format, it simply reports nothing.
 */
import { stat } from "node:fs/promises";
import { dirname } from "node:path";
import { cliSessionIdFor, desktopDataDir, desktopSessionsDir, listDesktopSessions } from "./desktop-sessions.ts";
import { platform } from "./platform/index.ts";
import { serial, watchDir } from "./watch.ts";

/** `at` is when that became true, in epoch ms. */
export type OnFocus = (sessionId: string | null, at: number) => void;

const FOCUS_LINE = /LocalSessions\.setFocusedSession: sessionId=(local_[A-Za-z0-9-]+|null)/g;

// The app logs a null between two focused sessions; only a null that lasts means "nothing in front".
const BLUR_DELAY_MS = 1500;

/** Last focus change in a chunk of log text: a `local_…` id, null, or undefined if there is none. */
export function lastFocus(text: string): string | null | undefined {
  let last: string | null | undefined;
  for (const match of text.matchAll(FOCUS_LINE)) last = match[1] === "null" ? null : match[1];
  return last;
}

/**
 * Calls `onFocus` with the CLI session id in front in the desktop app, or null when none, as the
 * app's log says it. Survives log rotation. `pollMs` is how often to look at the log even when
 * no change to it was reported. Returns a stop function.
 */
export function watchDesktopFocus(
  onFocus: OnFocus,
  { log = platform.desktopLog(desktopDataDir()), sessionsDir = desktopSessionsDir(), pollMs = 10_000 } = {},
) {
  let offset = 0;
  let partialLine = "";
  let blurTimer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;

  async function apply(value: string | null, at: number) {
    clearTimeout(blurTimer);
    if (value === null) blurTimer = setTimeout(() => onFocus(null, at + BLUR_DELAY_MS), BLUR_DELAY_MS);
    else {
      const sessionId = await cliSessionIdFor(value, sessionsDir);
      if (!stopped) onFocus(sessionId, at);
    }
  }

  const read = serial(async () => {
    try {
      const { size, mtimeMs } = await stat(log);
      if (size < offset) {
        offset = 0; // rotated: a fresh, shorter file
        partialLine = "";
      }
      if (size > offset) {
        const text = partialLine + (await Bun.file(log).slice(offset, size).text());
        offset = size;
        const end = text.lastIndexOf("\n") + 1;
        partialLine = text.slice(end); // finish this line on the next read
        const focus = lastFocus(text.slice(0, end));
        // The log's own time, not now: what an old log says last is old news.
        if (focus !== undefined && !stopped) await apply(focus, mtimeMs);
      }
    } catch {
      // No log yet (app never started): the next look may find it.
    }
  });

  read();
  const stopWatching = watchDir(dirname(log), read, { everyMs: pollMs, settleMs: 50 });
  return () => {
    stopped = true;
    stopWatching();
    clearTimeout(blurTimer);
  };
}

/**
 * Calls `onFocus` with the session the desktop app focused last, each time that becomes another
 * one, as the sessions' files say it. Never reports null: the files don't tell when the app lost
 * the focus. Returns a stop function.
 */
export function watchLastFocused(onFocus: OnFocus, { sessionsDir = desktopSessionsDir(), pollMs = 30_000 } = {}) {
  let reported: string | undefined;
  let stopped = false;

  const check = serial(async () => {
    try {
      const sessions = await listDesktopSessions(sessionsDir);
      const newest = sessions.reduce<(typeof sessions)[number] | undefined>((a, b) => (a && a.lastFocusedAt >= b.lastFocusedAt ? a : b), undefined);
      if (stopped || !newest || newest.lastFocusedAt === 0 || newest.localId === reported) return;
      reported = newest.localId;
      onFocus(newest.sessionId, newest.lastFocusedAt);
    } catch {
      // Unreadable for now: the next look may fare better.
    }
  });

  check();
  const stopWatching = watchDir(sessionsDir, check, { everyMs: pollMs, recursive: true, settleMs: 300 });
  return () => {
    stopped = true;
    stopWatching();
  };
}
