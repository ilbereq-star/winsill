import { watch, type FSWatcher } from "node:fs";

/**
 * Calls `onChange` shortly after anything changes under `dir`, once per burst of changes, and
 * every `everyMs` regardless: that timer is the safety net for a change the system doesn't
 * report, and what picks up a folder that didn't exist yet. Returns a stop function.
 */
export function watchDir(dir: string, onChange: () => void, { everyMs, recursive = false, settleMs = 100 }: { everyMs: number; recursive?: boolean; settleMs?: number }) {
  let watcher: FSWatcher | undefined;
  let settle: ReturnType<typeof setTimeout> | undefined;

  function changed() {
    clearTimeout(settle);
    settle = setTimeout(onChange, settleMs);
  }

  function attach() {
    if (watcher) return;
    try {
      watcher = watch(dir, { recursive }, changed);
      watcher.on("error", detach); // the folder went away; the timer attaches again when it is back
    } catch {
      // Not there yet.
    }
  }

  function detach() {
    watcher?.close();
    watcher = undefined;
  }

  attach();
  const timer = setInterval(() => {
    attach();
    onChange();
  }, everyMs);

  return () => {
    clearInterval(timer);
    clearTimeout(settle);
    detach();
  };
}

/**
 * `task` as a function that never runs twice at once: a call made while it runs becomes one more
 * run when it ends. For work started by change notifications, which come in bursts.
 */
export function serial(task: () => Promise<void>): () => void {
  let running = false;
  let again = false;
  return function run() {
    if (running) {
      again = true;
      return;
    }
    running = true;
    task()
      .catch(() => {})
      .finally(() => {
        running = false;
        if (again) {
          again = false;
          run();
        }
      });
  };
}
