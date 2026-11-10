import { readdirSync } from "node:fs";
import { win32 as path } from "node:path";
import type { Platform } from "./types.ts";

/** Folder of the desktop app's MSIX package (Store, WinGet): its name, then the 13-character publisher id. */
const MSIX_PACKAGE = /^Claude_[a-z0-9]{13}$/i;

export function windows(home: string, env: Record<string, string | undefined>): Platform {
  const roaming = env.APPDATA || path.join(home, "AppData", "Roaming");
  const local = env.LOCALAPPDATA || path.join(home, "AppData", "Local");
  const dataDir = path.join(local, "winsill");
  return {
    app: "winsill",
    dataDir,
    binDir: path.join(dataDir, "bin"),
    logDir: path.join(dataDir, "logs"),
    desktopDataDirs: () => desktopDataDirs(roaming, local, packageNames(path.join(local, "Packages"))),
    desktopLog: (dir) => path.join(dir, "logs", "main.log"),
    // Not `cmd /c start`: no shell parses the URL, and no console window flashes.
    openUrl: (url) => [path.join(env.SystemRoot || "C:\\Windows", "System32", "rundll32.exe"), "url.dll,FileProtocolHandler", url],
    lanHostName: async () => null,
    services: null,
  };
}

/**
 * The installer build keeps its data in %APPDATA%\Claude. The MSIX build may use that folder too,
 * or the private copy Windows gives the package: both have been seen, so both are candidates.
 */
export function desktopDataDirs(roaming: string, local: string, packages: string[]): string[] {
  const msix = packages
    .filter((name) => MSIX_PACKAGE.test(name))
    .map((name) => path.join(local, "Packages", name, "LocalCache", "Roaming", "Claude"));
  return [path.join(roaming, "Claude"), ...msix];
}

function packageNames(dir: string): string[] {
  try {
    return readdirSync(dir);
  } catch {
    return []; // no Packages folder: no MSIX apps
  }
}
