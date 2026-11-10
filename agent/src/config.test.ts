import { expect, test } from "bun:test";
import { join } from "node:path";
import { claudeConfigDir } from "./config.ts";

test("Claude Code's folder is ~/.claude unless CLAUDE_CONFIG_DIR moves it", () => {
  expect(claudeConfigDir({}, "/home/dev")).toBe(join("/home/dev", ".claude"));
  expect(claudeConfigDir({ CLAUDE_CONFIG_DIR: "" }, "/home/dev")).toBe(join("/home/dev", ".claude"));
  expect(claudeConfigDir({ CLAUDE_CONFIG_DIR: "/work/claude" }, "/home/dev")).toBe("/work/claude");
});
