import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { hooksInstalled, writeAtomic, writeHooks } from "./settings.ts";

let dir: string;
let settings: string;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "settings-"));
  settings = join(dir, "settings.json");
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

const read = (path = settings) => JSON.parse(readFileSync(path, "utf8"));

test("installing hooks keeps the rest of settings.json and leaves a backup of the old file", async () => {
  const original = JSON.stringify({ model: "opus", hooks: { Stop: [{ hooks: [{ type: "command", command: "echo mine" }] }] } });
  writeFileSync(settings, original);

  expect(await writeHooks({ install: true }, settings)).toBe(true);
  expect(read().model).toBe("opus");
  expect(read().hooks.Stop[0]).toEqual({ hooks: [{ type: "command", command: "echo mine" }] });
  expect(await hooksInstalled(settings)).toBe(true);

  const backups = readdirSync(dir).filter((name) => name.startsWith("settings.json.bak-"));
  expect(backups).toHaveLength(1);
  expect(readFileSync(join(dir, backups[0]!), "utf8")).toBe(original);
  expect(readdirSync(dir).filter((name) => name.endsWith(".tmp"))).toEqual([]);
});

test("nothing is written or backed up when the hooks are already as asked", async () => {
  writeFileSync(settings, JSON.stringify({ model: "opus" }));
  expect(await writeHooks({ install: false }, settings)).toBe(false);
  await writeHooks({ install: true }, settings);
  expect(await writeHooks({ install: true }, settings)).toBe(false);
  expect(readdirSync(dir)).toHaveLength(2); // settings.json and the one backup
});

test("installing creates settings.json when there is none, and removing gives the original back", async () => {
  expect(await writeHooks({ install: true }, settings)).toBe(true);
  expect(readdirSync(dir)).toEqual(["settings.json"]); // nothing to back up
  expect(await writeHooks({ install: false }, settings)).toBe(true);
  expect(read()).toEqual({});
});

test("a write that can't replace its target fails, leaving the target alone and no temporary file behind", async () => {
  const folder = join(dir, "folder"); // a file can't replace a folder that has something in it
  mkdirSync(folder);
  writeFileSync(join(folder, "kept"), "old");

  await expect(writeAtomic(folder, "new")).rejects.toThrow();
  expect(readFileSync(join(folder, "kept"), "utf8")).toBe("old");
  expect(readdirSync(dir)).toEqual(["folder"]);
});

test.skipIf(process.platform === "win32")("a symlinked settings.json stays a symlink, and the file keeps its mode", async () => {
  const real = join(dir, "dotfiles-settings.json");
  writeFileSync(real, "{}", { mode: 0o600 });
  symlinkSync(real, settings);

  await writeAtomic(settings, '{"a":1}\n');
  expect(read(real)).toEqual({ a: 1 });
  expect(statSync(real).mode & 0o777).toBe(0o600);
  expect(read()).toEqual({ a: 1 }); // through the link
});
