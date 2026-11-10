import { expect, test } from "bun:test";
import { normalizePath } from "./paths.ts";

test("Windows paths get one spelling: upper-case drive, backslashes, no trailing separator", () => {
  expect(normalizePath("c:/Users/dev/panel")).toBe(String.raw`C:\Users\dev\panel`);
  expect(normalizePath(String.raw`C:\Users\dev\panel` + "\\")).toBe(String.raw`C:\Users\dev\panel`);
  expect(normalizePath(String.raw`d:\work//mixed\\slashes/`)).toBe(String.raw`D:\work\mixed\slashes`);
  expect(normalizePath("c:/")).toBe("C:\\");
  expect(normalizePath("c:")).toBe("C:");
});

test("network paths keep their two leading backslashes", () => {
  expect(normalizePath(String.raw`\\server\share\repo` + "\\")).toBe(String.raw`\\server\share\repo`);
  expect(normalizePath(String.raw`\\server/share`)).toBe(String.raw`\\server\share`);
});

test("anything else passes through", () => {
  expect(normalizePath("/Users/dev/panel")).toBe("/Users/dev/panel");
  expect(normalizePath("/Users/dev/panel/")).toBe("/Users/dev/panel/");
  expect(normalizePath("relative/dir")).toBe("relative/dir");
  expect(normalizePath("")).toBe("");
});
