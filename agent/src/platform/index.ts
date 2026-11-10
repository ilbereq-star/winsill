import { homedir } from "node:os";
import { mac } from "./mac.ts";
import type { Platform } from "./types.ts";
import { windows } from "./windows.ts";

/** Anything that is not Windows gets the macOS behaviour, which is what the agent always assumed. */
export const platform: Platform = process.platform === "win32" ? windows(homedir(), process.env) : mac(homedir());

export type { Platform };
