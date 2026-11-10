import { $ } from "bun";
import type { PanelConfig } from "../../shared/protocol.ts";
import { existsSync } from "node:fs";
import { chmod, copyFile, mkdir, rename, rm } from "node:fs/promises";
import { dirname } from "node:path";
import { BIN_DIR, CLAUDE_PANEL_CONFIG, CLAUDE_PANEL_DB, CLAUDE_SETTINGS, CONFIG_FILE, DATA_DIR, LOG_DIR, loadConfig, panelUrls } from "./config.ts";
import { buildPanelHooks } from "./hooks-config.ts";
import { isLoaded, LEGACY_AGENT_LABEL, perchJobs, plistPath, remove, start, stop } from "./launchd.ts";
import { platform } from "./platform/index.ts";
import { writeHooks } from "./settings.ts";
import { Store } from "./store.ts";

/** What a release ships next to the `perch` binary, and setup copies into BIN_DIR. */
const SHIPPED = ["perch", "perch-overlay", "Perch_PerchCore.bundle"];

export async function setup({ pi }: { pi: boolean }) {
  if (platform.services !== "launchd") return setupHooks({ pi });

  const source = dirname(process.execPath);
  const missing = SHIPPED.filter((name) => !existsSync(`${source}/${name}`));
  if (missing.length > 0) {
    throw new Error(`${missing.join(", ")} not found next to ${process.execPath}; run setup from a release build`);
  }
  const jobs = perchJobs({ binDir: BIN_DIR, logDir: LOG_DIR }, { lan: pi });

  for (const job of jobs) await stop(job.label);
  // Its agent holds the same ports, and stopping it first leaves its database quiet for the import.
  const replacedClaudePanel = (await isLoaded(LEGACY_AGENT_LABEL)) || existsSync(plistPath(LEGACY_AGENT_LABEL));
  if (replacedClaudePanel) await remove(LEGACY_AGENT_LABEL);

  if (source !== BIN_DIR) await copyInto(source, BIN_DIR);
  if (!existsSync(CONFIG_FILE) && existsSync(CLAUDE_PANEL_CONFIG)) {
    await mkdir(DATA_DIR, { recursive: true });
    await copyFile(CLAUDE_PANEL_CONFIG, CONFIG_FILE);
    await chmod(CONFIG_FILE, 0o600);
  }
  const config = await loadConfig();
  const store = new Store(config.dbPath);
  const imported = store.importPinsFrom(CLAUDE_PANEL_DB);
  store.close();
  const hooksChanged = await writeHooks(buildPanelHooks(config.hookToken));
  // A strip started by hand would hold the single-instance lock, and the LaunchAgent's copy would quit.
  await $`pkill -x perch-overlay`.quiet().nothrow();
  for (const job of jobs) await start(job);

  console.log(`Perch is installed and running.
  binaries : ${BIN_DIR}
  data     : ${DATA_DIR}
  logs     : ${LOG_DIR}
  hooks    : ${hooksChanged ? "added to" : "already in"} ${CLAUDE_SETTINGS}`);
  if (replacedClaudePanel) console.log("  replaced claude-panel's agent (com.claude-panel.agent)");
  if (imported > 0) console.log(`  imported ${imported} pinned session${imported === 1 ? "" : "s"} from claude-panel`);
  if (pi) {
    const pairing: PanelConfig = { agentUrls: await panelUrls(), token: config.token };
    console.log(`\nThe agent also listens on your network. Pair the Pi with this config.json:\n${JSON.stringify(pairing, null, 2)}`);
  }
}

/**
 * Setup where nothing keeps the agent and a strip running yet, as on Windows: only the hooks,
 * which is all the agent needs from Claude Code.
 */
async function setupHooks({ pi }: { pi: boolean }) {
  if (pi) throw new Error("--pi is not available on this system yet");
  const config = await loadConfig();
  const hooksChanged = await writeHooks(buildPanelHooks(config.hookToken));
  console.log(`Claude Code now reports its events to ${platform.app}.
  hooks : ${hooksChanged ? "added to" : "already in"} ${CLAUDE_SETTINGS}
  data  : ${DATA_DIR}

The strip and starting at login are not built yet: start the agent yourself with \`${platform.app} agent\`.
A session that was already open may need a restart before it reports.`);
}

/** Each item lands under a temporary name, then replaces the old one in a single rename. */
async function copyInto(source: string, target: string) {
  await mkdir(target, { recursive: true });
  for (const name of SHIPPED) {
    const temporary = `${target}/.${name}.new`;
    await rm(temporary, { recursive: true, force: true });
    await $`cp -Rp ${`${source}/${name}`} ${temporary}`.quiet();
    await rm(`${target}/${name}`, { recursive: true, force: true });
    await rename(temporary, `${target}/${name}`);
  }
}

export async function uninstall({ purge }: { purge: boolean }) {
  const managed = platform.services === "launchd";
  if (managed) {
    for (const job of perchJobs({ binDir: BIN_DIR, logDir: LOG_DIR }, { lan: false })) await remove(job.label);
    await $`pkill -f ${`${BIN_DIR}/`}`.quiet().nothrow();
  }
  const hooksRemoved = await writeHooks(null);
  await rm(BIN_DIR, { recursive: true, force: true });

  const deleteData = purge || (process.stdin.isTTY && prompt(`Delete your pins and icons in ${DATA_DIR}? [y/N]`)?.trim().toLowerCase() === "y");
  if (deleteData) {
    await rm(DATA_DIR, { recursive: true, force: true });
    await rm(LOG_DIR, { recursive: true, force: true });
  }

  console.log(`${managed ? "Perch" : platform.app} is uninstalled.
  hooks : ${hooksRemoved ? "removed from" : "were not in"} ${CLAUDE_SETTINGS}
  data  : ${deleteData ? "deleted" : `kept in ${DATA_DIR}`}`);
  if (!managed) console.log(`\nIf the agent is running, stop it with Ctrl+C in its terminal.`);
  if (existsSync(CLAUDE_PANEL_DB)) {
    console.log("\nTo bring back claude-panel's agent: in claude-panel, `bun run setup`.");
  }
}
