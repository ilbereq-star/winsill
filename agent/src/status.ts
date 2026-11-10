import { PANEL_PORT } from "../../shared/protocol.ts";
import { BIN_DIR, LOG_DIR } from "./config.ts";
import { describe, perchJobs } from "./launchd.ts";
import { platform } from "./platform/index.ts";
import { hooksInstalled } from "./settings.ts";

export async function status(version: string) {
  const health = await fetch(`http://127.0.0.1:${PANEL_PORT}/health`, { signal: AbortSignal.timeout(2000) })
    .then((r) => r.json() as Promise<{ sessions: number }>)
    .catch(() => null);
  console.log(`${platform.app} ${version}
${await services()}
  hooks   : ${(await hooksInstalled()) ? "installed" : "not installed"}
  health  : ${health ? `answering, ${health.sessions} sessions` : `no answer on :${PANEL_PORT}`}
  logs    : ${LOG_DIR}`);
}

async function services() {
  if (platform.services !== "launchd") return `  agent   : not installed as a service; start it with \`${platform.app} agent\``;
  const [agent, overlay] = perchJobs({ binDir: BIN_DIR, logDir: LOG_DIR }, { lan: false });
  return `  agent   : ${await describe(agent!.label)}
  overlay : ${await describe(overlay!.label)}`;
}
