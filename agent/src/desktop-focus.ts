/**
 * Follows which session is in front in the Claude desktop app by tailing its log for
 * `LocalSessions.setFocusedSession` lines. The line is undocumented: if its format changes,
 * focus simply stays unknown.
 */
import { stat } from "node:fs/promises";
import { cliSessionIdFor, desktopDataDir, desktopSessionsDir } from "./desktop-sessions.ts";
import { platform } from "./platform/index.ts";

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
 * Calls `onFocus` with the CLI session id in front in the desktop app, or null when none.
 * Survives log rotation. Returns a stop function.
 */
export function watchDesktopFocus(
  onFocus: (sessionId: string | null) => void,
  { log = platform.desktopLog(desktopDataDir()), sessionsDir = desktopSessionsDir(), pollMs = 1000 } = {},
) {
  let offset = 0;
  let partialLine = "";
  let blurTimer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;

  async function apply(value: string | null) {
    clearTimeout(blurTimer);
    if (value === null) blurTimer = setTimeout(() => onFocus(null), BLUR_DELAY_MS);
    else onFocus(await cliSessionIdFor(value, sessionsDir));
  }

  (async () => {
    while (!stopped) {
      try {
        const { size } = await stat(log);
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
          if (focus !== undefined) await apply(focus);
        }
      } catch {
        // No log yet (app never started): keep polling.
      }
      await Bun.sleep(pollMs);
    }
  })();

  return () => {
    stopped = true;
    clearTimeout(blurTimer);
  };
}
