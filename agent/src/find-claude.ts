import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/**
 * The Claude Code CLI: on the PATH, or where its installer puts it. An agent started at login
 * doesn't always get the PATH a terminal has. Null if it isn't in either place.
 */
export function findClaude(env: Record<string, string | undefined> = process.env, home = homedir()): string | null {
  const onPath = Bun.which("claude", { PATH: env.PATH ?? env.Path ?? "" });
  if (onPath) return onPath;
  const installed = join(home, ".local", "bin", process.platform === "win32" ? "claude.exe" : "claude");
  return existsSync(installed) ? installed : null;
}
