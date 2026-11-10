import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { log } from "./log.ts";

let dir: string;
let file: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "log-"));
  file = join(dir, "logs", "agent.log");
});
afterEach(() => {
  log.toConsole();
  rmSync(dir, { recursive: true, force: true });
});

test("lines go to the file, each with its time, after what was already there", () => {
  log.toFile(file); // creates the folder
  log.info("first");
  log.toFile(file); // as a restart would
  log.error("second");

  const lines = readFileSync(file, "utf8").trimEnd().split("\n");
  expect(lines).toHaveLength(2);
  expect(lines[0]).toMatch(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z  first$/);
  expect(lines[1]).toEndWith("  second");
});

test("the log never grows past three files of 1 MB: the oldest lines are dropped", () => {
  log.toFile(file);
  const line = "x".repeat(1000);
  for (let i = 0; i < 4500; i++) log.info(`${i} ${line}`); // about 4.6 MB

  expect(readdirSync(join(dir, "logs")).sort()).toEqual(["agent.log", "agent.log.1", "agent.log.2"]);
  for (const name of ["agent.log", "agent.log.1", "agent.log.2"]) {
    expect(statSync(join(dir, "logs", name)).size).toBeLessThanOrEqual(1024 * 1024);
  }
  expect(readFileSync(file, "utf8")).toContain("  4499 x"); // the newest line is in the current file
  expect(readFileSync(`${file}.2`, "utf8")).not.toContain("  0 x"); // the oldest ones are gone
});

test("a log that can't be written doesn't stop the agent", () => {
  log.toFile(file);
  rmSync(join(dir, "logs"), { recursive: true });
  writeFileSync(join(dir, "logs"), "now a file, so agent.log can't be created under it");
  expect(() => log.info("lost")).not.toThrow();
  expect(existsSync(file)).toBe(false);
});
