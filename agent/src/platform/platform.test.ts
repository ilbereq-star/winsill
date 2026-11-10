import { expect, test } from "bun:test";
import { mac } from "./mac.ts";
import { desktopDataDirs, windows } from "./windows.ts";

const HOME = String.raw`C:\Users\dev`;
const ENV = { APPDATA: String.raw`C:\Users\dev\AppData\Roaming`, LOCALAPPDATA: String.raw`C:\Users\dev\AppData\Local`, SystemRoot: String.raw`C:\Windows` };

test("macOS keeps the paths the overlay and the LaunchAgents already use", () => {
  const p = mac("/Users/dev");
  expect(p.app).toBe("perch");
  expect(p.dataDir).toBe("/Users/dev/Library/Application Support/perch");
  expect(p.binDir).toBe("/Users/dev/Library/Application Support/perch/bin");
  expect(p.logDir).toBe("/Users/dev/Library/Logs/perch");
  expect(p.desktopDataDirs()).toEqual(["/Users/dev/Library/Application Support/Claude"]);
  expect(p.desktopLog(p.desktopDataDirs()[0]!)).toBe("/Users/dev/Library/Logs/Claude/main.log");
  expect(p.openUrl("claude://x")).toEqual(["open", "claude://x"]);
  expect(p.services).toBe("launchd");
});

test("Windows keeps its own data under %LOCALAPPDATA% and has no setup yet", () => {
  const p = windows(HOME, ENV);
  expect(p.app).toBe("winsill");
  expect(p.dataDir).toBe(String.raw`C:\Users\dev\AppData\Local\winsill`);
  expect(p.binDir).toBe(String.raw`C:\Users\dev\AppData\Local\winsill\bin`);
  expect(p.logDir).toBe(String.raw`C:\Users\dev\AppData\Local\winsill\logs`);
  expect(p.desktopLog(String.raw`C:\Users\dev\AppData\Roaming\Claude`)).toBe(String.raw`C:\Users\dev\AppData\Roaming\Claude\logs\main.log`);
  expect(p.services).toBeNull();
});

test("Windows falls back to the usual folders when the environment doesn't name them", () => {
  expect(windows(HOME, {}).dataDir).toBe(String.raw`C:\Users\dev\AppData\Local\winsill`);
});

test("Windows opens a URL without a shell, so nothing in it is parsed as a command", () => {
  const url = "claude://code/continue?session=local_abc&x=1";
  expect(windows(HOME, ENV).openUrl(url)).toEqual([String.raw`C:\Windows\System32\rundll32.exe`, "url.dll,FileProtocolHandler", url]);
});

test("the desktop app's data may be in %APPDATA% or in its MSIX package's private copy", () => {
  const packages = ["Claude_pzs8sxrjxfjjc", "Microsoft.WindowsTerminal_8wekyb3d8bbwe", "ClaudeNotes_pzs8sxrjxfjjc", "Claude_short"];
  expect(desktopDataDirs(ENV.APPDATA, ENV.LOCALAPPDATA, packages)).toEqual([
    String.raw`C:\Users\dev\AppData\Roaming\Claude`,
    String.raw`C:\Users\dev\AppData\Local\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude`,
  ]);
  expect(desktopDataDirs(ENV.APPDATA, ENV.LOCALAPPDATA, [])).toEqual([String.raw`C:\Users\dev\AppData\Roaming\Claude`]);
});
