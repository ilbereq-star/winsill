/**
 * Live view of all sessions and pins, persisted through Store. Listeners hear only
 * about changes the panel can see, so a burst of identical hook events yields one message.
 */
import { basename } from "node:path";
import { DEFAULT_ICON, type PanelSession, type SessionIcon } from "../../shared/protocol.ts";
import { applyHookEvent, reconcile, type AgentEntry, type HookEvent, type SessionRecord } from "./state.ts";
import type { Pin, Store } from "./store.ts";

export type HubChange =
  | { type: "session"; session: PanelSession }
  | { type: "removed"; id: string }
  | { type: "focus"; id: string | null };
export type HubListener = (change: HubChange) => void;

const DAY_MS = 24 * 60 * 60 * 1000;
const VISIBLE: (keyof SessionRecord)[] = ["name", "cwd", "state", "activity", "live", "stateSince"];

export class SessionHub {
  private records = new Map<string, SessionRecord>();
  private pins: Map<string, Pin>;
  private listeners = new Set<HubListener>();
  /** Session in front in the desktop app; not persisted. */
  focusedId: string | null = null;

  constructor(
    private store: Store,
    private now: () => number = Date.now,
  ) {
    for (const rec of store.loadSessions()) this.records.set(rec.id, rec);
    this.pins = store.loadPins();
  }

  subscribe(listener: HubListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  has(id: string) {
    return this.records.has(id);
  }

  get(id: string): PanelSession | undefined {
    const rec = this.records.get(id);
    return rec && this.toPanel(rec);
  }

  /** All known sessions; the panel decides which to show. */
  snapshot(): PanelSession[] {
    return [...this.records.values()].map((rec) => this.toPanel(rec));
  }

  handleHook(event: HookEvent) {
    this.commit(applyHookEvent(this.records.get(event.session_id), event, this.now()));
  }

  handleAgents(agents: AgentEntry[]) {
    for (const rec of reconcile(this.records, agents, this.now())) this.commit(rec);
  }

  /** Starts tracking a session that is not running, e.g. a desktop session about to be pinned. No-op if known. */
  adopt(id: string, name: string | null, cwd: string, since: number) {
    if (this.records.has(id)) return;
    this.commit({ id, name, cwd, state: "ended", activity: null, live: false, pid: null, stateSince: since, lastEventAt: 0 });
  }

  /** False if the session is unknown. */
  setPinned(id: string, pinned: boolean): boolean {
    const rec = this.records.get(id);
    if (!rec) return false;
    if (pinned) this.store.pin(id);
    else this.store.unpin(id);
    this.pins = this.store.loadPins();
    this.emit({ type: "session", session: this.toPanel(rec) });
    return true;
  }

  /** Pins the session if needed. False if the session is unknown. */
  setIcon(id: string, icon: SessionIcon): boolean {
    const rec = this.records.get(id);
    if (!rec) return false;
    this.store.setIcon(id, icon);
    this.pins = this.store.loadPins();
    this.emit({ type: "session", session: this.toPanel(rec) });
    return true;
  }

  setFocused(id: string | null) {
    if (id === this.focusedId) return;
    this.focusedId = id;
    this.emit({ type: "focus", id });
  }

  /** Forgets ended, unpinned sessions whose state is older than `maxAgeMs`. */
  prune(maxAgeMs = DAY_MS) {
    const cutoff = this.now() - maxAgeMs;
    for (const rec of this.records.values()) {
      if (rec.state === "ended" && !this.pins.has(rec.id) && rec.stateSince < cutoff) {
        this.records.delete(rec.id);
        this.store.deleteSession(rec.id);
        this.emit({ type: "removed", id: rec.id });
      }
    }
  }

  private commit(next: SessionRecord) {
    const prev = this.records.get(next.id);
    this.records.set(next.id, next);
    const visible = !prev || VISIBLE.some((k) => prev[k] !== next[k]);
    // Most hook events only move lastEventAt, which is of no use after a restart: not worth a write.
    if (visible || prev?.pid !== next.pid) this.store.saveSession(next);
    if (visible) this.emit({ type: "session", session: this.toPanel(next) });
  }

  private emit(change: HubChange) {
    for (const listener of this.listeners) listener(change);
  }

  private toPanel(rec: SessionRecord): PanelSession {
    const pin = this.pins.get(rec.id);
    return {
      id: rec.id,
      name: rec.name ?? basename(rec.cwd),
      cwd: rec.cwd,
      state: rec.state,
      activity: rec.activity,
      live: rec.live,
      stateSince: rec.stateSince,
      pinned: pin !== undefined,
      position: pin?.position ?? null,
      icon: pin?.icon ?? DEFAULT_ICON,
    };
  }
}
