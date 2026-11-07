/**
 * What the endpoint place says, read from one dashboard frame: its loaded
 * rows, the queue, the room for a second model, the recent requests and the
 * activity. Pure: a frame in, words out.
 *
 * Every fact is the proxy's (`DashboardSnapshot`): a resident's state and
 * its flag line, an active request's prompt progress, the queue, the
 * second slot's room in the daemon's own sentence (`secondary_slot.detail`),
 * what gglib did to a request in the proxy's own words
 * (`Intervention.word`), the counters. State words are
 * `components/status/words.ts`'s. The sentences around them are this
 * place's own, listed in its README.
 *
 * @module places/endpoint/lines
 */

import { RESIDENCY_WORDS } from '../../components/status/words';
import type { ServerViewModel } from '../../hooks/useServers';
import { rateWords, requestRate } from '../../readings/throughput';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';
import type { ResidencyState } from '../../types/generated/ResidencyState';
import { formatCount } from '../../utils/format';

/** One loaded model as its row says it. */
export interface LoadedRow {
  modelId: number;
  name: string;
  /** Its llama-server's port, which a chat opened from the row is handed. */
  port: number;
  state: ResidencyState;
  /** The state as a word: "Reading 61%" while a prompt is being read, else its residency word. */
  word: string;
  /** How far a prompt has been read, 0–100, while one is; else `null`. */
  reading: number | null;
  /** The launch's one line (`LaunchFacts.flag_line`); empty when none was said. */
  flagLine: string;
}

/** How far the prompt of a request on `modelName` has been read, while one is being read. */
function readingOf(snapshot: DashboardSnapshot, modelName: string): number | null {
  const reading = snapshot.active_connections.find(
    (c) => c.model_name === modelName && c.phase === 'processing_prompt' && (c.prompt_total ?? 0) > 0,
  );
  if (!reading || reading.prompt_total == null) return null;
  return Math.round(((reading.prompt_processed ?? 0) / reading.prompt_total) * 100);
}

/** The loaded rows, primary first as the proxy lists them. */
export function loadedRows(snapshot: DashboardSnapshot | null): LoadedRow[] {
  if (!snapshot) return [];
  return snapshot.admission.slots.map((slot) => {
    const state = slot.launch?.state ?? slot.state;
    const reading = readingOf(snapshot, slot.model_name);
    return {
      modelId: slot.model_id,
      name: slot.model_name,
      port: slot.port,
      state,
      word: reading === null ? RESIDENCY_WORDS[state] : `${RESIDENCY_WORDS.reading} ${reading}%`,
      reading,
      flagLine: slot.launch?.flag_line ?? '',
    };
  });
}

/**
 * What is in memory: the dashboard's rows while it has spoken, else the
 * running servers, each Loaded, since a model stays in memory with the
 * endpoint off (the dashboard is idle then). The endpoint place and the
 * tray both read it.
 */
export function residentRows(snapshot: DashboardSnapshot | null, servers: ServerViewModel[]): LoadedRow[] {
  if (snapshot) return loadedRows(snapshot);
  return servers.map((s) => ({
    modelId: s.modelId,
    name: s.modelName,
    port: s.port,
    state: 'loaded',
    word: RESIDENCY_WORDS.loaded,
    reading: null,
    flagLine: '',
  }));
}

/** A wait in whole seconds as `gglib proxy dashboard` says it: "4s", "1m 5s". */
function waitWords(ms: number): string {
  const secs = Math.floor(ms / 1000);
  return secs < 60 ? `${secs}s` : `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

/** One line per model with requests queued for a slot: who, how many, how long the oldest has waited. */
export function queuedLines(snapshot: DashboardSnapshot | null): string[] {
  return (snapshot?.admission.queued ?? []).map(
    (q) =>
      `${q.model_name} ${RESIDENCY_WORDS.waiting.toLowerCase()} for a slot · ${q.waiting} ${q.waiting === 1 ? 'request' : 'requests'} · ${waitWords(q.oldest_wait_ms)}`,
  );
}

/**
 * The daemon's sentence for the room a second model has ("Room for one
 * more, about 9.0 GiB."), while the second slot is free and its headroom
 * known; else `null`.
 */
export function roomLine(snapshot: DashboardSnapshot | null): string | null {
  const slot = snapshot?.admission.secondary_slot;
  return slot?.headroom_bytes == null ? null : slot.detail;
}

/** One recent request as Recent says it. */
export interface RecentRow {
  key: string;
  /** When it was recorded, as a clock time. */
  time: string;
  model: string;
  /** "812 tokens · 34 t/s", only the figures recorded; empty with none. */
  figures: string;
  /** What gglib did to it, in the proxy's words; empty when nothing. */
  done: string;
}

/** How many of the recent requests Recent shows, as `gglib proxy dashboard` does. */
const RECENT_SHOWN = 5;

/**
 * The last `shown` requests (five, unless asked for more), newest first;
 * the proxy sends them oldest first.
 */
export function recentRows(snapshot: DashboardSnapshot | null, shown = RECENT_SHOWN): RecentRow[] {
  const recent = snapshot?.recent_requests ?? [];
  return recent
    .slice(-shown)
    .reverse()
    .map((r, i) => {
      const rate = requestRate(r);
      return {
        key: `${r.recorded_at_secs}-${i}`,
        time: new Date(r.recorded_at_secs * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
        model: r.model_name,
        figures: [
          r.completion_tokens != null ? `${formatCount(r.completion_tokens)} ${r.completion_tokens === 1 ? 'token' : 'tokens'}` : null,
          rate !== null ? rateWords(rate) : null,
        ]
          .filter(Boolean)
          .join(' · '),
        done: r.interventions.map((i) => i.word).join(' · '),
      };
    });
}

/** The share of prompt tokens served from the cache, as a whole percent; `null` before any was measured. */
export function cacheReusePercent(snapshot: DashboardSnapshot | null): number | null {
  const usage = snapshot?.cache?.usage;
  if (!usage || usage.reporting_requests === 0 || usage.prompt_tokens === 0) return null;
  return Math.round((usage.cached_tokens / usage.prompt_tokens) * 100);
}

/** The dashboard's other sections as short sentences, each only when it has something to say. */
export function activitySentences(snapshot: DashboardSnapshot | null): string[] {
  if (!snapshot) return [];
  const plural = (n: number, one: string, many: string) => `${formatCount(n)} ${n === 1 ? one : many}`;
  const reuse = cacheReusePercent(snapshot);
  const trips = Object.values(snapshot.per_model_defects).reduce((sum, d) => sum + d.loop_guard_trips, 0);
  const ignored = snapshot.sampling_audit.client_fields_discarded;
  return [
    reuse !== null ? `Prompt cache ${reuse}% reused` : null,
    snapshot.tool_repairs_succeeded > 0 ? `${plural(snapshot.tool_repairs_succeeded, 'tool call', 'tool calls')} repaired` : null,
    trips > 0 ? plural(trips, 'loop-guard trip', 'loop-guard trips') : null,
    ignored > 0 ? `${plural(ignored, 'client sampling field', 'client sampling fields')} ignored` : null,
  ].filter((s): s is string => s !== null);
}
