import { chmod, copyFile, realpath, rename, rm, stat } from "node:fs/promises";
import { CLAUDE_SETTINGS } from "./config.ts";
import { hasPanelHooks, mergeHooks, type Settings } from "./hooks-config.ts";
import { platform } from "./platform/index.ts";

async function readSettings(path: string): Promise<Settings> {
  const file = Bun.file(path);
  return (await file.exists()) ? await file.json() : {};
}

/** Adds or removes our hooks in Claude Code's settings.json, backing the file up first. False when nothing changed. */
export async function writeHooks({ install }: { install: boolean }, path = CLAUDE_SETTINGS): Promise<boolean> {
  const exists = await Bun.file(path).exists();
  const before = await readSettings(path);
  const after = mergeHooks(before, { install });
  if (JSON.stringify(after) === JSON.stringify(before)) return false;
  if (exists) await copyFile(path, `${path}.bak-${platform.app}-${Date.now()}`);
  await writeAtomic(path, JSON.stringify(after, null, 2) + "\n");
  return true;
}

export async function hooksInstalled(path = CLAUDE_SETTINGS) {
  return hasPanelHooks(await readSettings(path));
}

/**
 * Replaces `path` in one step: whoever reads it sees the old text or the new, never half of it,
 * and a crash mid-write leaves the old file. A symlink stays one (its target is replaced), and
 * the file keeps its permissions.
 */
export async function writeAtomic(path: string, text: string) {
  const target = await realpath(path).catch(() => path); // not there yet
  const mode = (await stat(target).catch(() => null))?.mode;
  const temporary = `${target}.${process.pid}.tmp`;
  try {
    await Bun.write(temporary, text);
    if (mode !== undefined) await chmod(temporary, mode & 0o777);
    await replace(temporary, target);
  } catch (error) {
    await rm(temporary, { force: true });
    throw error;
  }
}

/** Windows refuses to replace a file while another process has it open; that lasts a moment, so try again. */
async function replace(from: string, to: string) {
  for (let attempt = 1; ; attempt++) {
    try {
      return await rename(from, to);
    } catch (error) {
      const busy = ["EPERM", "EBUSY", "EACCES"].includes((error as NodeJS.ErrnoException)?.code ?? "");
      if (!busy || attempt === 10) throw error;
      await Bun.sleep(50);
    }
  }
}
