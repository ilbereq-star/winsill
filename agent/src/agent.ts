import { join } from "node:path";
import { PANEL_PORT } from "../../shared/protocol.ts";
import { HOOK_PORT, loadConfig, LOG_DIR, panelUrls } from "./config.ts";
import { watchDesktopFocus, watchLastFocused } from "./desktop-focus.ts";
import { SessionHub } from "./hub.ts";
import { log } from "./log.ts";
import { platform } from "./platform/index.ts";
import { startPoller } from "./poller.ts";
import { startServers } from "./server.ts";
import { Store } from "./store.ts";

/** Runs until it is told to stop. `lan` opens the WebSocket to the network, for a claude-panel Pi. */
export async function runAgent({ lan, version }: { lan: boolean; version: string }) {
  // launchd collects the agent's output into a file; elsewhere, with no terminal to print to, the agent keeps its own.
  if (!process.stdout.isTTY && platform.services !== "launchd") log.toFile(join(LOG_DIR, "agent.log"));

  const config = await loadConfig();
  const store = new Store(config.dbPath);
  const hub = new SessionHub(store);
  const servers = startServers({
    hub,
    token: config.token,
    hookToken: config.hookToken,
    hookPort: HOOK_PORT,
    panelPort: PANEL_PORT,
    panelHost: lan ? "0.0.0.0" : "127.0.0.1",
    version,
  });
  const stopPoller = startPoller(hub);
  // Two sources say which session is in front; the one that speaks of the later moment wins.
  let focusedAt = 0;
  const focus = (id: string | null, at: number) => {
    if (at < focusedAt) return;
    focusedAt = at;
    hub.setFocused(id);
  };
  const stopFocus = [watchDesktopFocus(focus), watchLastFocused(focus)];
  const pruneTimer = setInterval(() => hub.prune(), 10 * 60 * 1000);

  hub.subscribe((change) => {
    if (change.type === "removed") return log.info(`${change.id.slice(0, 8)}  removed`);
    if (change.type === "focus") return log.info(`focus → ${change.id ? hub.get(change.id)?.name ?? change.id : "none"}`);
    const s = change.session;
    const flags = `${s.live ? "" : " (not live)"}${s.pinned ? " ★" : ""}`;
    log.info(`${s.id.slice(0, 8)}  ${s.state.padEnd(16)} ${s.name}${flags}  ${s.activity ?? ""}`);
  });

  const urls = lan ? await panelUrls() : [`ws://127.0.0.1:${PANEL_PORT}/ws`];
  log.info(`${platform.app} agent ${version}`);
  log.info(`  hooks : http://127.0.0.1:${HOOK_PORT}/hook`);
  log.info(`  panel : ${urls.join("  ")}`);
  log.info(`  data  : ${config.dbPath}`);

  function shutdown() {
    clearInterval(pruneTimer);
    stopPoller();
    for (const stop of stopFocus) stop();
    servers.stop();
    store.close();
    process.exit(0);
  }
  // SIGHUP is what closing the agent's terminal or console window sends.
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"] as const) process.on(signal, shutdown);
}
