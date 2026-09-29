import { useCallback, useEffect, useRef, useState } from 'react';
import { getTransport } from '../../services/transport';
import type { RunInfo } from '../../types/generated/RunInfo';

/** How often the page asks which runs are going. */
export const ACTIVITY_POLL_MS = 3000;

/**
 * Where the New marks are kept: this browser only, a JSON object of
 * conversation id to when the reply it marks ended (ms since the epoch).
 */
export const UNREAD_STORAGE_KEY = 'gglib.chat.unread';

type Marks = Record<string, number>;

/** The marks as stored now; `null` when storage cannot be read. */
function readStored(): Marks | null {
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(UNREAD_STORAGE_KEY) ?? '{}');
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(([id, at]) => /^\d+$/.test(id) && typeof at === 'number'),
    );
  } catch {
    return null;
  }
}

const readMarks = (): Marks => readStored() ?? {};

function writeMarks(marks: Marks): void {
  try {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify(marks));
  } catch {
    // Storage unavailable: the marks last this visit only.
  }
}

const isLive = (run: RunInfo) => run.status === 'queued' || run.status === 'in_progress';

/**
 * A conversation list as the daemon sent it, and when it was asked for.
 * Never a list built or patched on this side.
 */
export interface FetchedList {
  ids: readonly number[];
  /** When the fetch was sent, in ms since the epoch. */
  askedAt: number;
}

export interface ConversationActivity {
  /** Conversations with an agent run going. */
  running: ReadonlySet<number>;
  /** Conversations whose reply ended while another was on screen. */
  unread: ReadonlySet<number>;
}

/**
 * Which conversations are Running and which are New.
 *
 * Running: `GET /api/runs` lists a live agent run in it. New: a run in it
 * ended, seen here, while it was not the conversation on screen; showing
 * it clears the mark. A run that had already ended when the page first
 * asked marks nothing: it may have been read anywhere.
 */
export function useConversationActivity(
  activeId: number | null,
  fetched: FetchedList | null = null,
  pollMs: number = ACTIVITY_POLL_MS,
): ConversationActivity {
  const [running, setRunning] = useState<ReadonlySet<number>>(() => new Set());
  const [marks, setMarks] = useState<Marks>(readMarks);
  const marksRef = useRef(marks);

  /**
   * Change the marks as another tab may have left them: read what is
   * stored, change that, and write it back only if it changed. A tab that
   * wrote from its own copy would restore a mark another tab had cleared.
   */
  const change = useCallback((edit: (marks: Marks) => Marks) => {
    const current = readStored() ?? marksRef.current;
    const next = edit(current);
    if (JSON.stringify(next) !== JSON.stringify(current)) writeMarks(next);
    marksRef.current = next;
    setMarks(next);
  }, []);

  // Another tab's change: take it as it stands.
  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key !== UNREAD_STORAGE_KEY && event.key !== null) return;
      const stored = readStored();
      if (!stored) return;
      marksRef.current = stored;
      setMarks(stored);
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  // A mark is dropped only on the daemon's word: its conversation is absent
  // from a list the daemon just sent, and the mark predates the asking (a
  // conversation this tab has not heard of is not a deleted one).
  useEffect(() => {
    if (!fetched) return;
    const listed = new Set(fetched.ids.map(String));
    change((current) =>
      Object.fromEntries(
        Object.entries(current).filter(([id, at]) => listed.has(id) || at >= fetched.askedAt),
      ),
    );
  }, [fetched, change]);
  const activeRef = useRef(activeId);
  /** When each conversation was last left: a reply ended before that was seen. */
  const leftAt = useRef(new Map<number, number>());

  useEffect(() => {
    const previous = activeRef.current;
    if (previous !== null && previous !== activeId) leftAt.current.set(previous, Date.now());
    activeRef.current = activeId;
    if (activeId === null) return;
    change((current) => {
      const next = { ...current };
      delete next[activeId];
      return next;
    });
  }, [activeId, change]);

  useEffect(() => {
    let cancelled = false;
    /** Runs this page has seen, and whether each was seen ended. */
    let seen: Map<string, boolean> | null = null;

    const poll = async () => {
      let runs: RunInfo[];
      try {
        runs = await getTransport().listRuns();
      } catch {
        return;
      }
      if (cancelled) return;
      const agent = runs.filter((run) => run.kind === 'agent' && run.conversation_id != null);
      const live = new Set(agent.filter(isLive).map((run) => run.conversation_id as number));
      setRunning((prev) => (prev.size === live.size && [...live].every((id) => prev.has(id)) ? prev : live));

      const first = seen === null;
      const known: Map<string, boolean> = seen ?? new Map();
      const ended: Marks = {};
      for (const run of agent) {
        const cid = run.conversation_id as number;
        if (isLive(run)) {
          known.set(run.id, false);
          continue;
        }
        if (known.get(run.id) === true) continue;
        const watched = known.has(run.id) || !first;
        known.set(run.id, true);
        const at = run.finished_at_ms ?? Date.now();
        if (!watched || cid === activeRef.current) continue;
        if (at <= (leftAt.current.get(cid) ?? -Infinity)) continue;
        ended[cid] = at;
      }
      seen = known;
      if (Object.keys(ended).length > 0) change((current) => ({ ...current, ...ended }));
    };

    void poll();
    const timer = setInterval(() => void poll(), pollMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [pollMs, change]);

  const unread = new Set(Object.keys(marks).map(Number));
  return { running, unread };
}
