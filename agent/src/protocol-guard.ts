import type { ClientMessage, SessionIcon } from "../../shared/protocol.ts";
import type { HookEvent } from "./state.ts";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const CHARACTER = /^[a-z0-9-]{1,32}$/;
const COLOR = /^#[0-9a-f]{6}$/i;

export function isSessionId(value: unknown): value is string {
  return typeof value === "string" && UUID.test(value);
}

function isIcon(value: unknown): value is SessionIcon {
  const v = value as Partial<SessionIcon> | null;
  return (
    typeof v === "object" &&
    v !== null &&
    typeof v.character === "string" &&
    CHARACTER.test(v.character) &&
    typeof v.color === "string" &&
    COLOR.test(v.color)
  );
}

/** Parses an untrusted panel message: null unless it is exactly one of the whitelisted ClientMessages. */
export function parseClientMessage(raw: string): ClientMessage | null {
  let msg: Record<string, unknown>;
  try {
    msg = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof msg !== "object" || msg === null) return null;
  if (msg.type === "library") return { type: "library" };
  if (!isSessionId(msg.id)) return null;

  switch (msg.type) {
    case "open":
      return { type: "open", id: msg.id };
    case "pin":
      return typeof msg.pinned === "boolean" ? { type: "pin", id: msg.id, pinned: msg.pinned } : null;
    case "icon":
      return isIcon(msg.icon)
        ? { type: "icon", id: msg.id, icon: { character: msg.icon.character, color: msg.icon.color.toLowerCase() } }
        : null;
    default:
      return null;
  }
}

const MAX_EVENT_NAME = 64;

/**
 * An untrusted hook body as the event the state machine takes: null unless it names a session
 * by its id and says which event it is. A missing cwd becomes "": some events may not carry one.
 */
export function parseHookEvent(body: unknown): HookEvent | null {
  const event = body as Record<string, unknown> | null;
  if (typeof event !== "object" || event === null || Array.isArray(event)) return null;
  if (!isSessionId(event.session_id)) return null;
  const name = event.hook_event_name;
  if (typeof name !== "string" || name.length === 0 || name.length > MAX_EVENT_NAME) return null;
  return { ...event, session_id: event.session_id, hook_event_name: name, cwd: typeof event.cwd === "string" ? event.cwd : "" };
}
