/**
 * Session state machine. Pure: no I/O, and callers pass the clock.
 *
 * Hook events are the authority: instant and detailed. Polls of `claude agents --json`
 * only correct state that has gone stale (no hook for GRACE_MS), and are the only
 * reliable signal that a session process has died.
 */
import { basename } from "node:path";
import type { SessionState } from "../../shared/protocol.ts";

export interface HookEvent {
  hook_event_name: string;
  session_id: string;
  cwd: string;
  /** Set on tool events fired by a subagent rather than the main thread. */
  agent_id?: string;
  [field: string]: unknown;
}

/** One entry of `claude agents --json`. */
export interface AgentEntry {
  sessionId: string;
  name?: string;
  cwd: string;
  kind?: string;
  pid?: number;
  status?: "busy" | "waiting" | "idle";
  startedAt: number;
}

export interface SessionRecord {
  id: string;
  /** Session title; only `claude agents` knows it. */
  name: string | null;
  /** Fixed at first sight: hooks report the shell's current directory, which moves. */
  cwd: string;
  state: SessionState;
  activity: string | null;
  live: boolean;
  pid: number | null;
  stateSince: number;
  /** Time of the last hook event; 0 if only the poll has seen the session. */
  lastEventAt: number;
}

/** How long hook-derived state wins over a contradicting poll. */
export const GRACE_MS = 10_000;

/** Tools that block until the user answers. */
const ASKS_USER: Record<string, string> = {
  AskUserQuestion: "Asked you a question",
  ExitPlanMode: "Plan ready for review",
};

const MAX_ACTIVITY = 48;

function newRecord(id: string, cwd: string, now: number, state: SessionState = "idle"): SessionRecord {
  return { id, name: null, cwd, state, activity: null, live: true, pid: null, stateSince: now, lastEventAt: now };
}

export function applyHookEvent(prev: SessionRecord | undefined, e: HookEvent, now: number): SessionRecord {
  const rec: SessionRecord = { ...(prev ?? newRecord(e.session_id, e.cwd, now)), live: true, lastEventAt: now };
  const to = (state: SessionState, activity: string | null) => moveTo(rec, state, activity, now);
  // Background subagents keep calling tools after the turn ends, under the parent's session_id.
  const afterTurn = Boolean(e.agent_id) && (rec.state === "done" || rec.state === "error");

  switch (e.hook_event_name) {
    case "SessionStart":
      return e.source === "compact" ? rec : to("idle", null);

    case "UserPromptSubmit":
      return to("working", "Thinking…");

    case "PreToolUse": {
      const tool = String(e.tool_name ?? "");
      if (tool in ASKS_USER) return to("needs_input", ASKS_USER[tool]!);
      return afterTurn ? rec : to("working", describeTool(tool, e.tool_input));
    }

    case "PostToolUse":
    case "PostToolUseFailure":
      return afterTurn ? rec : to("working", describeTool(String(e.tool_name ?? ""), e.tool_input));

    case "Notification":
      switch (e.notification_type) {
        case "permission_prompt":
          return to("needs_permission", oneLine(e.message) || "Needs your permission");
        case "elicitation_dialog":
        case "agent_needs_input":
          return to("needs_input", oneLine(e.message) || "Waiting for you");
        case "idle_prompt": // still "working" here means the Stop was missed
          return rec.state === "working" || rec.state === "idle" ? to("done", null) : rec;
        default:
          return rec;
      }

    case "Stop":
      return to("done", null);

    case "StopFailure":
      return to("error", oneLine(e.error) || "Error");

    case "SubagentStart":
      return to("working", `Subagent: ${e.agent_type ?? "task"}`);

    case "SubagentStop": // background subagents may finish after the turn; don't revive a finished session
      return rec;

    case "PreCompact":
      return to("compacting", "Compacting context");

    case "PostCompact":
      return to(e.trigger === "manual" ? "done" : "working", null);

    case "SessionEnd":
      return { ...to("ended", null), live: false, pid: null };

    default:
      return rec;
  }
}

/** Tile label such as "Bash: npm test", "Edit: state.ts" or "github: create_issue". */
export function describeTool(tool: string, input: unknown): string {
  const i = (input ?? {}) as Record<string, unknown>;
  const str = (v: unknown) => (typeof v === "string" ? v : "");
  let label: string;

  if (tool.startsWith("mcp__")) {
    const [, server, ...name] = tool.split("__");
    label = `${server}: ${name.join("__")}`;
  } else if (str(i.file_path)) {
    label = `${tool}: ${basename(str(i.file_path))}`;
  } else {
    const detail = str(i.command) || str(i.pattern) || str(i.description) || str(i.query) || str(i.url);
    label = detail ? `${tool}: ${detail}` : tool;
  }
  return truncate(oneLine(label), MAX_ACTIVITY);
}

/** Merges one poll into `records`; returns only the records that changed. */
export function reconcile(
  records: ReadonlyMap<string, SessionRecord>,
  agents: AgentEntry[],
  now: number,
): SessionRecord[] {
  const changed: SessionRecord[] = [];
  const listed = new Set<string>();

  for (const agent of agents) {
    listed.add(agent.sessionId);
    const prev = records.get(agent.sessionId);
    const next = reconcileOne(prev, agent, now);
    if (!prev || !sameRecord(prev, next)) changed.push(next);
  }

  for (const rec of records.values()) {
    if (rec.live && !listed.has(rec.id) && now - rec.lastEventAt > GRACE_MS) {
      changed.push({ ...moveTo(rec, "ended", null, now), live: false, pid: null });
    }
  }
  return changed;
}

function reconcileOne(prev: SessionRecord | undefined, agent: AgentEntry, now: number): SessionRecord {
  const polled = stateFromStatus(agent.status);

  if (!prev) {
    return { ...newRecord(agent.sessionId, agent.cwd, now, polled), name: agent.name ?? null, pid: agent.pid ?? null, lastEventAt: 0 };
  }

  const rec: SessionRecord = { ...prev, name: agent.name ?? prev.name, cwd: agent.cwd, pid: agent.pid ?? prev.pid, live: true };

  if (prev.state === "ended") return moveTo(rec, polled, null, now);
  if (now - prev.lastEventAt <= GRACE_MS) return rec;

  // Hooks have gone quiet: correct only clear contradictions. Busy doesn't wake a finished turn
  // (done or error): a background task keeps the poll at busy after the turn ends, and the
  // Stop or StopFailure hook is authoritative. It still clears idle and a stale prompt (needs_*).
  const { state } = rec;
  const finished = state === "done" || state === "error";
  if (agent.status === "idle" && (state === "working" || state === "compacting")) return moveTo(rec, "done", null, now);
  if (agent.status === "busy" && state !== "working" && state !== "compacting" && !finished) return moveTo(rec, "working", null, now);
  if (agent.status === "waiting" && state === "working") return moveTo(rec, "needs_input", "Waiting for you", now);
  return rec;
}

function stateFromStatus(status: AgentEntry["status"]): SessionState {
  if (status === "busy") return "working";
  if (status === "waiting") return "needs_input";
  return "idle";
}

function moveTo(rec: SessionRecord, state: SessionState, activity: string | null, now: number): SessionRecord {
  return { ...rec, state, activity, stateSince: state === rec.state ? rec.stateSince : now };
}

function sameRecord(a: SessionRecord, b: SessionRecord) {
  return (Object.keys(a) as (keyof SessionRecord)[]).every((k) => a[k] === b[k]);
}

function oneLine(value: unknown) {
  return String(value ?? "").replace(/\s+/g, " ").trim();
}

function truncate(text: string, max: number) {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}
