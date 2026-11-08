/**
 * Two servers: hooks on loopback only, and the panel WebSocket behind the pairing token,
 * on loopback unless a Pi needs it on the network. Keeping them apart means the
 * network-facing port can never inject hook events.
 */
import { timingSafeEqual } from "node:crypto";
import { basename } from "node:path";
import type { LibraryEntry, ServerMessage } from "../../shared/protocol.ts";
import { listDesktopSessions, type DesktopSession } from "./desktop-sessions.ts";
import type { SessionHub } from "./hub.ts";
import { openSession, type OpenResult } from "./open-session.ts";
import { parseClientMessage } from "./protocol-guard.ts";
import type { HookEvent } from "./state.ts";

export interface ServerOptions {
  hub: SessionHub;
  token: string;
  hookPort: number;
  panelPort: number;
  panelHost?: string;
  /** Replaceable so tests don't open real sessions or read the real desktop app. */
  open?: (id: string) => Promise<OpenResult>;
  desktopSessions?: () => Promise<DesktopSession[]>;
}

const TOPIC = "panel";
const LIBRARY_SIZE = 100;

export function startServers({
  hub,
  token,
  hookPort,
  panelPort,
  panelHost = "127.0.0.1",
  open = openSession,
  desktopSessions = () => listDesktopSessions(),
}: ServerOptions) {
  async function library(): Promise<LibraryEntry[]> {
    return (await desktopSessions())
      .filter((s) => !s.archived)
      .slice(0, LIBRARY_SIZE)
      .map((s) => ({
        id: s.sessionId,
        name: s.title ?? hub.get(s.sessionId)?.name ?? basename(s.cwd),
        cwd: s.cwd,
        lastActivityAt: s.lastActivityAt,
        pinned: hub.get(s.sessionId)?.pinned ?? false,
      }));
  }

  /** Lets the panel pin a desktop session whose process isn't running, so the agent never saw it. */
  async function adoptFromDesktop(id: string) {
    const session = (await desktopSessions()).find((s) => s.sessionId === id);
    if (session) hub.adopt(session.sessionId, session.title, session.cwd, session.lastActivityAt);
  }

  const hookServer = Bun.serve({
    hostname: "127.0.0.1",
    port: hookPort,
    async fetch(req) {
      const { pathname } = new URL(req.url);
      if (req.method !== "POST" || pathname !== "/hook") return new Response("not found", { status: 404 });
      try {
        const event = (await req.json()) as HookEvent;
        if (typeof event?.session_id === "string" && typeof event.hook_event_name === "string") hub.handleHook(event);
      } catch {
        return new Response("bad json", { status: 400 });
      }
      return new Response(null, { status: 204 }); // an empty 2xx tells Claude Code "no decision"
    },
  });

  const panelServer = Bun.serve({
    hostname: panelHost,
    port: panelPort,
    fetch(req, server) {
      const url = new URL(req.url);
      if (url.pathname === "/health") return Response.json({ ok: true, sessions: hub.snapshot().length });
      if (url.pathname !== "/ws") return new Response("not found", { status: 404 });
      if (!sameSecret(url.searchParams.get("token"), token)) return new Response("unauthorized", { status: 401 });
      if (server.upgrade(req)) return;
      return new Response("expected a WebSocket", { status: 400 });
    },
    websocket: {
      maxPayloadLength: 4 * 1024,
      open(ws) {
        ws.subscribe(TOPIC);
        send(ws, { type: "snapshot", sessions: hub.snapshot(), focusedId: hub.focusedId });
      },
      async message(ws, raw) {
        // An async handler's rejection is unhandled and kills the agent: answer it instead.
        try {
          const msg = parseClientMessage(String(raw));
          if (!msg) return send(ws, { type: "error", message: "invalid message" });
          if (msg.type === "library") return send(ws, { type: "library", sessions: await library() });
          if (!hub.has(msg.id) && msg.type !== "open") await adoptFromDesktop(msg.id);
          if (!hub.has(msg.id)) return send(ws, { type: "error", message: "unknown session" });

          switch (msg.type) {
            case "open": {
              const result = await open(msg.id);
              // The desktop log confirms focus a moment later; show it right away.
              if (result.ok) hub.setFocused(msg.id);
              else send(ws, { type: "error", message: result.error });
              break;
            }
            case "pin":
              hub.setPinned(msg.id, msg.pinned);
              break;
            case "icon":
              hub.setIcon(msg.id, msg.icon);
              break;
          }
        } catch (error) {
          console.error(`panel message failed: ${error instanceof Error ? error.message : String(error)}`);
          send(ws, { type: "error", message: "internal error" }); // details stay in the log, off the network port
        }
      },
      close(ws) {
        ws.unsubscribe(TOPIC);
      },
    },
  });

  const unsubscribe = hub.subscribe((change) => panelServer.publish(TOPIC, JSON.stringify(change satisfies ServerMessage)));

  return {
    hookServer,
    panelServer,
    stop() {
      unsubscribe();
      hookServer.stop(true);
      panelServer.stop(true);
    },
  };
}

function send(ws: { send(data: string): unknown }, message: ServerMessage) {
  ws.send(JSON.stringify(message));
}

/** Constant-time, so response timing doesn't leak the token. */
function sameSecret(given: string | null, expected: string) {
  if (!given) return false;
  const a = Buffer.from(given);
  const b = Buffer.from(expected);
  return a.length === b.length && timingSafeEqual(a, b);
}
