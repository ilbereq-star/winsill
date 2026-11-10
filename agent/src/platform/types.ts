/** What differs between the operating systems the agent runs on. */
export interface Platform {
  /** Name of the CLI and of the folders it owns. */
  app: string;
  /** Holds config.json, with the pairing token, and the SQLite store. */
  dataDir: string;
  /** Where setup copies the binaries. */
  binDir: string;
  logDir: string;
  /** Folders where the Claude desktop app may keep its data, the usual one first. */
  desktopDataDirs(): string[];
  /** The desktop app's main.log, given the data folder it uses. */
  desktopLog(dataDir: string): string;
  /** Command that opens `url` in the app registered for its scheme. */
  openUrl(url: string): string[];
  /** A name other devices on the network can resolve to this machine, if it has one. */
  lanHostName(): Promise<string | null>;
  /** What keeps the agent and the strip running after setup; null where setup is not built yet. */
  services: "launchd" | null;
}
