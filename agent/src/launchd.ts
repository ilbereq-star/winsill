import { $ } from "bun";
import { mkdir, rm } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname } from "node:path";

export interface Job {
  label: string;
  program: string[];
  /** "after-crash" leaves the job stopped when it exits cleanly, as after Quit Perch. */
  keepAlive: "always" | "after-crash";
  log: string;
}

export const LEGACY_AGENT_LABEL = "com.claude-panel.agent";

const HOME = homedir();
/** A function: Windows has no getuid, and every command imports this file. */
const domain = () => `gui/${process.getuid!()}`;
// launchd starts processes with a bare PATH; the agent runs `claude`.
const PATH = [`${HOME}/.local/bin`, `${HOME}/.bun/bin`, "/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"].join(":");

export function perchJobs({ binDir, logDir }: { binDir: string; logDir: string }, { lan }: { lan: boolean }): Job[] {
  return [
    {
      label: "com.elrincondeldev.perch.agent",
      program: [`${binDir}/perch`, "agent", ...(lan ? ["--lan"] : [])],
      keepAlive: "always",
      log: `${logDir}/agent.log`,
    },
    {
      label: "com.elrincondeldev.perch.overlay",
      program: [`${binDir}/perch-overlay`],
      keepAlive: "after-crash",
      log: `${logDir}/overlay.log`,
    },
  ];
}

const xml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

export function plist(job: Job): string {
  const keepAlive = job.keepAlive === "always" ? "<true/>" : "<dict><key>SuccessfulExit</key><false/></dict>";
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>${xml(job.label)}</string>
  <key>ProgramArguments</key>
  <array>
${job.program.map((arg) => `    <string>${xml(arg)}</string>`).join("\n")}
  </array>
  <key>EnvironmentVariables</key>
  <dict><key>PATH</key><string>${xml(PATH)}</string></dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key>${keepAlive}
  <key>StandardOutPath</key><string>${xml(job.log)}</string>
  <key>StandardErrorPath</key><string>${xml(job.log)}</string>
</dict>
</plist>
`;
}

export function plistPath(label: string) {
  return `${HOME}/Library/LaunchAgents/${label}.plist`;
}

/** Writes the job's plist and (re)starts it. */
export async function start(job: Job) {
  await mkdir(dirname(job.log), { recursive: true });
  await Bun.write(plistPath(job.label), plist(job));
  await stop(job.label);
  await $`launchctl bootstrap ${domain()} ${plistPath(job.label)}`.quiet();
}

/** Stops the job, if loaded, and waits until it is gone: bootstrapping again any sooner fails with "Input/output error". */
export async function stop(label: string) {
  await $`launchctl bootout ${domain()}/${label}`.quiet().nothrow();
  for (let i = 0; i < 50; i++) {
    if (!(await isLoaded(label))) return;
    await Bun.sleep(100);
  }
  throw new Error(`${label} did not stop`);
}

export async function remove(label: string) {
  await stop(label);
  await rm(plistPath(label), { force: true });
}

export async function isLoaded(label: string) {
  return (await $`launchctl print ${domain()}/${label}`.quiet().nothrow()).exitCode === 0;
}

/** "running (pid 123)", "not running" or "not installed". */
export async function describe(label: string) {
  const info = await $`launchctl print ${domain()}/${label}`.quiet().nothrow();
  if (info.exitCode !== 0) return "not installed";
  const pid = info.stdout.toString().match(/^\s*pid = (\d+)$/m)?.[1];
  return pid ? `running (pid ${pid})` : "not running";
}
