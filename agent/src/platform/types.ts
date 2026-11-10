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
  /** The app registered to open `scheme://` links, null if none is, undefined where that can't be told. */
  urlHandler(scheme: string): Promise<string | null | undefined>;
  /** How Claude Code hooks reach the agent: through curl, or posted by Claude Code itself. */
  hookStyle: "command" | "http";
  /** What keeps the agent and the strip running after setup; null where that is not built yet. */
  services: "launchd" | null;
}
