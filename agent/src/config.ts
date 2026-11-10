/**
 * Where the agent lives on disk. The data dir holds config.json with the pairing token (created
 * on first run, mode 600; on Windows the folder is already private to the user), the SQLite store, and bin/ with the binaries setup installs.
 * On macOS the overlay reads the token from here too (Sources/PerchCore/AgentConfig.swift).
 */
import { randomBytes } from "node:crypto";
import { chmod, mkdir } from "node:fs/promises";
import { homedir, networkInterfaces } from "node:os";
import { join } from "node:path";
import { PANEL_PORT } from "../../shared/protocol.ts";
import { platform } from "./platform/index.ts";

export const HOOK_PORT = 7777;

const HOME = homedir();
export const DATA_DIR = platform.dataDir;
export const BIN_DIR = platform.binDir;
export const LOG_DIR = platform.logDir;
export const CLAUDE_CONFIG_DIR = claudeConfigDir(process.env, HOME);
export const CLAUDE_SETTINGS = join(CLAUDE_CONFIG_DIR, "settings.json");
/** One <pid>.json per running Claude Code process (internal to the CLI). */
export const CLAUDE_LIVE_SESSIONS = join(CLAUDE_CONFIG_DIR, "sessions");
/** The agent Perch replaces on macOS: setup imports its pins and reuses its pairing token, so a paired Pi keeps working. */
const CLAUDE_PANEL_DIR = `${HOME}/Library/Application Support/claude-panel`;
export const CLAUDE_PANEL_DB = `${CLAUDE_PANEL_DIR}/panel.db`;
export const CLAUDE_PANEL_CONFIG = `${CLAUDE_PANEL_DIR}/config.json`;
export const CONFIG_FILE = join(DATA_DIR, "config.json");

/** Claude Code's own folder: ~/.claude unless CLAUDE_CONFIG_DIR moves it. */
export function claudeConfigDir(env: Record<string, string | undefined>, home: string) {
  return env.CLAUDE_CONFIG_DIR || join(home, ".claude");
}

/**
 * WebSocket URLs other devices on the LAN can use to reach this machine: its IPv4 addresses
 * (Wi-Fi/Ethernet first), then its network name. IPs come first because some routers drop
 * mDNS between WiFi clients; the name still works if the IP changes later.
 */
export async function panelUrls(): Promise<string[]> {
  const ips = Object.entries(networkInterfaces())
    .sort(([a], [b]) => Number(b.startsWith("en")) - Number(a.startsWith("en")))
    .flatMap(([, addresses]) => addresses ?? [])
    .filter((a) => a.family === "IPv4" && !a.internal)
    .map((a) => a.address);
  const host = await platform.lanHostName();
  return [...ips, ...(host ? [host] : [])].map((address) => `ws://${address}:${PANEL_PORT}/ws`);
}

export interface Config {
  dataDir: string;
  dbPath: string;
  /** Pairs the strip, or a panel, with the agent's WebSocket. */
  token: string;
  /** Sent by the hooks in settings.json. A secret of its own: that file is the one people sync and share. */
  hookToken: string;
}

const newToken = () => randomBytes(24).toString("base64url");

/** What config.json holds, without creating it: null before the agent's first run. */
export async function savedConfig(): Promise<{ token?: string; hookToken?: string } | null> {
  const file = Bun.file(CONFIG_FILE);
  return (await file.exists()) ? await file.json() : null;
}

/** The agent's config, creating the data dir and any missing token. */
export async function loadConfig(): Promise<Config> {
  const dataDir = DATA_DIR;
  await mkdir(dataDir, { recursive: true });

  const path = CONFIG_FILE;
  const saved = (await savedConfig()) ?? {};

  if (!saved.token || !saved.hookToken) {
    saved.token ||= newToken();
    saved.hookToken ||= newToken();
    await Bun.write(path, JSON.stringify(saved, null, 2) + "\n");
    await chmod(path, 0o600);
  }

  return { dataDir, dbPath: join(dataDir, `${platform.app}.db`), token: saved.token, hookToken: saved.hookToken };
}
