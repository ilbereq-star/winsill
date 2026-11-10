const DRIVE = /^[a-z]:(?=[\\/]|$)/i;

/**
 * One spelling per Windows path: `c:/Users/me/` and `C:\Users\me` are the same folder, and the
 * sources the agent merges (hooks, `claude agents`, the desktop app) don't agree on which to
 * report. Other paths pass through untouched.
 */
export function normalizePath(path: string): string {
  const unc = path.startsWith("\\\\");
  if (!unc && !DRIVE.test(path)) return path;
  let fixed = path.replace(/[\\/]+/g, "\\");
  if (unc) fixed = `\\${fixed}`; // the line above folded the two leading backslashes into one
  else fixed = fixed[0]!.toUpperCase() + fixed.slice(1);
  return fixed.length > 3 && fixed.endsWith("\\") ? fixed.slice(0, -1) : fixed;
}
