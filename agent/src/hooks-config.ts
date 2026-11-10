import { HOOK_PORT } from "./config.ts";
import { platform } from "./platform/index.ts";

/**
 * Hook events the agent needs to track session state. PermissionRequest is left out on purpose:
 * a hook there can answer the prompt for the user, and Notification's permission_prompt already
 * says a session is waiting.
 */
export const PANEL_EVENTS = [
  "SessionStart",
  "UserPromptSubmit",
  "PreToolUse",
  "PostToolUse",
  "PostToolUseFailure",
  "Notification",
  "Stop",
  "StopFailure",
  "SubagentStart",
  "SubagentStop",
  "PreCompact",
  "PostCompact",
  "SessionEnd",
] as const;

export const HOOK_URL = `http://127.0.0.1:${HOOK_PORT}/hook`;

type Handler = { type?: string; url?: string; command?: unknown; [k: string]: unknown };
type MatcherGroup = { matcher?: string; hooks: Handler[] };
export type PanelHooks = Record<string, MatcherGroup[]>;
export type Settings = { hooks?: PanelHooks; [k: string]: unknown };

/**
 * The `hooks` block for settings.json, forwarding every event to the agent with the token it
 * asks for. Hooks stay synchronous so events arrive in order, with a short timeout so an agent
 * that hangs can't hold a session back.
 *
 * - "command", with curl: a hook that fails stays silent thanks to `|| true`.
 * - "http": Claude Code posts the event itself. On Windows a command hook goes through Git Bash
 *   or PowerShell, which is slow, can flash a console window and has no curl at a fixed path.
 *   A failed http hook is a non-blocking error that the transcript doesn't show.
 *   Claude Code skips http hooks on SessionStart, so that event is left out: a session's start
 *   shows through the file its process writes (live-sessions.ts).
 */
export function buildPanelHooks(token: string, style = platform.hookStyle): PanelHooks {
  const events = style === "http" ? PANEL_EVENTS.filter((event) => event !== "SessionStart") : PANEL_EVENTS;
  const authorization = `Bearer ${token}`;
  const handler: Handler =
    style === "http"
      ? { type: "http", url: HOOK_URL, headers: { Authorization: authorization }, timeout: 2 }
      : {
          type: "command",
          command: `/usr/bin/curl -s -m 1 -H 'Content-Type: application/json' -H 'Authorization: ${authorization}' --data-binary @- ${HOOK_URL} >/dev/null 2>&1 || true`,
          timeout: 2,
        };
  return Object.fromEntries(events.map((event) => [event, [{ hooks: [handler] }]]));
}

/** True for handlers installed by this project, whatever their style or token. */
export function isPanelHook(handler: Handler) {
  if (handler.type === "http") return handler.url === HOOK_URL;
  return handler.type === "command" && typeof handler.command === "string" && handler.command.includes(HOOK_URL);
}

/** `settings` with our hooks removed, then `ours` added when given; everything else stays as it was. */
export function mergeHooks(settings: Settings, ours: PanelHooks | null): Settings {
  const hooks: PanelHooks = {};
  for (const [event, groups] of Object.entries(settings.hooks ?? {})) {
    const kept = groups
      .map((group) => ({ ...group, hooks: group.hooks.filter((h) => !isPanelHook(h)) }))
      .filter((group) => group.hooks.length > 0);
    if (kept.length > 0) hooks[event] = kept;
  }
  for (const [event, groups] of Object.entries(ours ?? {})) hooks[event] = [...(hooks[event] ?? []), ...groups];
  const { hooks: _, ...rest } = settings;
  return Object.keys(hooks).length > 0 ? { ...rest, hooks } : rest;
}

export function hasPanelHooks(settings: Settings) {
  return Object.values(settings.hooks ?? {}).some((groups) => groups.some((group) => group.hooks.some(isPanelHook)));
}

/** "current" when installing `ours` would change nothing; "outdated" for ours in another style or with another token. */
export function hooksState(settings: Settings, ours: PanelHooks): "current" | "outdated" | "missing" {
  if (!hasPanelHooks(settings)) return "missing";
  return JSON.stringify(mergeHooks(settings, ours)) === JSON.stringify(settings) ? "current" : "outdated";
}
