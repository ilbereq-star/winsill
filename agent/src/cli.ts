import { platform } from "./platform/index.ts";

export type Command =
  | { name: "agent"; lan: boolean }
  | { name: "setup"; pi: boolean }
  | { name: "status" }
  | { name: "uninstall"; purge: boolean }
  | { name: "version" }
  | { name: "help"; error?: string };

export const USAGE = `Usage: ${platform.app} <command>

  setup [--pi]         install the agent, the strip and the Claude Code hooks, and start them
                       --pi also lets a claude-panel Raspberry Pi connect over the LAN
  status               show what is installed and running
  uninstall [--purge]  remove everything; --purge also deletes pins and icons without asking
  agent [--lan]        run the agent in the foreground (what setup keeps running)
  version              print the version`;

const FLAGS: Record<string, Record<string, string>> = {
  agent: { "--lan": "lan" },
  setup: { "--pi": "pi" },
  status: {},
  uninstall: { "--purge": "purge" },
};

export function parseCommand(args: string[]): Command {
  const [name, ...rest] = args;
  if (name === undefined || name === "help" || name === "--help" || name === "-h") return { name: "help" };
  if (name === "version" || name === "--version" || name === "-v") return { name: "version" };
  const allowed = FLAGS[name];
  if (!allowed) return { name: "help", error: `unknown command: ${name}` };

  const command: Record<string, unknown> = { name };
  for (const key of Object.values(allowed)) command[key] = false;
  for (const arg of rest) {
    const key = allowed[arg];
    if (!key) return { name: "help", error: `unknown option for ${name}: ${arg}` };
    command[key] = true;
  }
  return command as Command;
}
