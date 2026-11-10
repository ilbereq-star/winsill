import { readdirSync } from "node:fs";
import { win32 as path } from "node:path";
import type { Platform } from "./types.ts";

/** Folder of the desktop app's MSIX package (Store, WinGet): its name, then the 13-character publisher id. */
const MSIX_PACKAGE = /^Claude_[a-z0-9]{13}$/i;

export function windows(home: string, env: Record<string, string | undefined>): Platform {
  const roaming = env.APPDATA || path.join(home, "AppData", "Roaming");
  const local = env.LOCALAPPDATA || path.join(home, "AppData", "Local");
  const system32 = path.join(env.SystemRoot || "C:\\Windows", "System32");
  const dataDir = path.join(local, "winsill");
  return {
    app: "winsill",
    dataDir,
    binDir: path.join(dataDir, "bin"),
    logDir: path.join(dataDir, "logs"),
    desktopDataDirs: () => desktopDataDirs(roaming, local, packageNames(path.join(local, "Packages"))),
    desktopLog: (dir) => path.join(dir, "logs", "main.log"),
    // Not `cmd /c start`: no shell parses the URL, and no console window flashes.
    openUrl: (url) => [path.join(system32, "rundll32.exe"), "url.dll,FileProtocolHandler", url],
    lanHostName: async () => null,
    async urlHandler(scheme) {
      if (!/^[a-z][a-z0-9+.-]*$/i.test(scheme)) return null;
      const key = `HKCR\\${scheme}\\shell\\open\\command`;
      const query = Bun.spawn([path.join(system32, "reg.exe"), "query", key, "/ve"], { stdout: "pipe", stderr: "ignore", windowsHide: true });
      const output = await new Response(query.stdout).text();
      return (await query.exited) === 0 ? parseRegDefault(output) : null;
    },
    hookStyle: "http",
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

/** The value in `reg query <key> /ve` output, whose line reads `    (Default)    REG_SZ    <value>` with a localized name. */
export function parseRegDefault(output: string): string | null {
  return output.match(/^\s+.+?\s+REG_(?:EXPAND_)?SZ\s+(.+?)\s*$/m)?.[1] ?? null;
}

function packageNames(dir: string): string[] {
  try {
    return readdirSync(dir);
  } catch {
    return []; // no Packages folder: no MSIX apps
  }
}
