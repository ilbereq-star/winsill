import { $ } from "bun";
import type { Platform } from "./types.ts";

export function mac(home: string): Platform {
  const dataDir = `${home}/Library/Application Support/perch`;
  return {
    app: "perch",
    dataDir,
    binDir: `${dataDir}/bin`,
    logDir: `${home}/Library/Logs/perch`,
    desktopDataDirs: () => [`${home}/Library/Application Support/Claude`],
    desktopLog: () => `${home}/Library/Logs/Claude/main.log`,
    openUrl: (url) => ["open", url],
    /** The Bonjour name, which still resolves after the Mac's IP changes. */
    async lanHostName() {
      const name = (await $`scutil --get LocalHostName`.quiet().nothrow().text()).trim();
      return name ? `${name}.local` : null;
    },
    services: "launchd",
  };
}
