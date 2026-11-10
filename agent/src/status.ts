import { PANEL_PORT } from "../../shared/protocol.ts";
import { BIN_DIR, LOG_DIR, savedConfig } from "./config.ts";
import { buildPanelHooks } from "./hooks-config.ts";
import { describe, perchJobs } from "./launchd.ts";
import { platform } from "./platform/index.ts";
import { installedHooks } from "./settings.ts";

const HOOKS = {
  current: "installed",
  outdated: `installed, but not as this version writes them: run \`${platform.app} setup\` again`,
  missing: "not installed",
};

export async function agentHealth(): Promise<{ sessions: number } | null> {
  return fetch(`http://127.0.0.1:${PANEL_PORT}/health`, { signal: AbortSignal.timeout(2000) })
    .then((r) => r.json() as Promise<{ sessions: number }>)
    .catch(() => null);
}

/** How settings.json's hooks compare with the ones this agent would install. Creates nothing: with no token yet, any hooks of ours are outdated. */
export async function hooksStatus() {
  return installedHooks(buildPanelHooks((await savedConfig())?.hookToken ?? ""));
}

export async function status(version: string) {
  const health = await agentHealth();
  const hooks = await hooksStatus();
  console.log(`${platform.app} ${version}
${await services()}
  hooks   : ${HOOKS[hooks]}
  health  : ${health ? `answering, ${health.sessions} sessions` : `no answer on :${PANEL_PORT}`}
  logs    : ${LOG_DIR}`);
}

async function services() {
  if (platform.services !== "launchd") return `  agent   : not installed as a service; start it with \`${platform.app} agent\``;
  const [agent, overlay] = perchJobs({ binDir: BIN_DIR, logDir: LOG_DIR }, { lan: false });
  return `  agent   : ${await describe(agent!.label)}
  overlay : ${await describe(overlay!.label)}`;
}
