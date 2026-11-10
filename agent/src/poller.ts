import { CLAUDE_LIVE_SESSIONS } from "./config.ts";
import { findClaude } from "./find-claude.ts";
import type { SessionHub } from "./hub.ts";
import { readLiveSessions } from "./live-sessions.ts";
import { log } from "./log.ts";
import type { AgentEntry } from "./state.ts";
import { serial, watchDir } from "./watch.ts";

export interface PollerOptions {
  /** The CLI's folder of running sessions. */
  dir?: string;
  /** How often to look even when nothing seemed to change: this is what notices a process that died. */
  intervalMs?: number;
  /** How often to ask `claude agents --json` instead, while the files can't tell. */
  cliIntervalMs?: number;
  /** Replaceable so tests don't start the CLI. */
  askCli?: () => Promise<AgentEntry[]>;
}

/**
 * Tells the hub which sessions are running and whether each is busy: when one of their files
 * changes, and every `intervalMs`. Returns a stop function.
 */
export function startPoller(hub: SessionHub, { dir = CLAUDE_LIVE_SESSIONS, intervalMs = 3000, cliIntervalMs = 15_000, askCli = claudeAgents }: PollerOptions = {}) {
  let stopped = false;
  let lastCli = -Infinity;
  let lastError = "";

  const poll = serial(async () => {
    try {
      let agents: AgentEntry[] | null = await readLiveSessions(dir);
      if (!agents && Date.now() - lastCli >= cliIntervalMs) {
        lastCli = Date.now();
        agents = await askCli();
      }
      if (agents && !stopped) hub.handleAgents(agents);
      lastError = "";
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message !== lastError) log.error(`poll failed: ${message}`); // log each distinct failure once
      lastError = message;
    }
  });

  poll();
  const stopWatching = watchDir(dir, poll, { everyMs: intervalMs });
  return () => {
    stopped = true;
    stopWatching();
  };
}

async function claudeAgents(): Promise<AgentEntry[]> {
  // windowsHide: an agent without a console must not make one appear for the CLI.
  const claude = Bun.spawn([findClaude() ?? "claude", "agents", "--json"], { stdout: "pipe", stderr: "ignore", windowsHide: true });
  const output = await new Response(claude.stdout).text();
  const code = await claude.exited;
  if (code !== 0) throw new Error(`claude agents exited with ${code}`);
  return JSON.parse(output);
}
