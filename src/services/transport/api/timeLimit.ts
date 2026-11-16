/**
 * How long a request to the daemon may go unanswered, and the one `fetch`
 * every request to it is made through.
 *
 * A daemon stuck inside a handler (a probe that never returns) once left a
 * read waiting for ever, and the health probe deciding whether the service is
 * up answered neither way. The limits, all here:
 *
 * - a read (GET) ends after `READ_LIMIT_MS`: its body included through
 *   `get`, its headers through `apiFetch`, whose caller reads the body;
 * - the health probe after `HEALTH_LIMIT_MS`;
 * - a stream (`stream: true`) is limited until its response headers come,
 *   and never after, however long it runs;
 * - a mutation (POST, PUT, PATCH, DELETE) has none: one that took effect and
 *   answered late would be reported as failed;
 * - a caller's `timeoutMs` replaces any of these, for a mutation that wants
 *   one or a read that legitimately takes longer.
 *
 * A request that runs out rejects with a `TransportError` coded `TIMEOUT`,
 * whose message is the sentence a page shows. It is not an `AbortError`, so a
 * reader that drops its own aborts still says it; a caller's own signal
 * still aborts as an abort.
 *
 * The clock starts at `fetch`: the desktop app's wait for the daemon's
 * address (`get_embedded_api_info`) comes before it and is not counted.
 */

import { TransportError } from '../errors';

/** A read that is not a stream: the whole reply, headers and body. */
const READ_LIMIT_MS = 30_000;

/** The health probe, asked every 2 s. One with no answer in this time is asked again at once (`useDaemonReachable`). */
export const HEALTH_LIMIT_MS = 1_500;

/** A stream, until its response headers come. */
const STREAM_HEADERS_LIMIT_MS = 30_000;

/**
 * A read the daemon answers from the Hugging Face Hub. gglib-hf gives each
 * Hub call 30 s and up to four tries, and these routes make several calls in
 * turn (a repo's listing is one per folder; a pick reads each variant's
 * header), so 30 s here would cut off a slow Hub the daemon is still
 * waiting on, and say less than the daemon's own error would.
 */
export const HUB_READ_LIMIT_MS = 5 * 60_000;

/**
 * A request the daemon forwards to the paired machine through its streaming
 * client (an image from that machine's store, a far run's events). The
 * daemon may first ask what that machine serves (up to 3 s), then allows it
 * 20 s to connect and 45 s of silence (`gglib-app-services`
 * `remote/far_proxy.rs`), so the headers can take about 70 s.
 */
export const FAR_STREAMED_LIMIT_MS = 90_000;

/** What a caller says about its request's time limit. */
export interface TimeLimitOptions {
  /** The reply is a stream: limited until its headers come, never after. */
  stream?: boolean;
  /** The caller's own limit, in milliseconds; on a stream it limits the headers. */
  timeoutMs?: number;
}

/** How long a request may take, and to what point; `null` is no limit. */
type Limit = { ms: number; until: 'headers' | 'reply' } | null;

/** The limit a request made with `method` and `options` is held to. */
export function limitFor(method: string, { stream = false, timeoutMs }: TimeLimitOptions = {}): Limit {
  const until = stream ? 'headers' : 'reply';
  if (timeoutMs !== undefined) return { ms: timeoutMs, until };
  if (stream) return { ms: STREAM_HEADERS_LIMIT_MS, until };
  const verb = method.toUpperCase();
  if (verb === 'GET' || verb === 'HEAD') return { ms: READ_LIMIT_MS, until };
  return null;
}

/** The error a request that ran out of `ms` rejects with. */
function timedOut(ms: number): TransportError {
  return new TransportError('TIMEOUT', `gglib did not answer in ${spoken(ms)}.`, { timeoutMs: ms });
}

/** `ms` as a person says it: "30 seconds", "1 minute", "5 minutes". */
function spoken(ms: number): string {
  const [n, unit] = ms >= 60_000 && ms % 60_000 === 0 ? [ms / 60_000, 'minute'] : [ms / 1000, 'second'];
  return `${n} ${unit}${n === 1 ? '' : 's'}`;
}

/**
 * `fetch` `url` and hand the response to `read`, within `limit`: with
 * `until: 'reply'` the limit runs until `read` is done, with `'headers'`
 * until `fetch` resolves. A `read` that hands the response back unread is
 * limited only to its headers, whatever `until` says; its caller reads the
 * body unbounded.
 *
 * The caller's signal in `init` aborts the request at any point, the body
 * included, and its rejection is passed on as it came.
 */
export async function fetchWithin<T>(
  url: string,
  init: RequestInit,
  limit: Limit,
  read: (response: Response) => Promise<T>,
): Promise<T> {
  if (!limit) return read(await fetch(url, init));

  const own = init.signal ?? undefined;
  const controller = new AbortController();
  const follow = () => controller.abort(own?.reason);
  if (own?.aborted) follow();
  else own?.addEventListener('abort', follow, { once: true });

  const timeout = timedOut(limit.ms);
  let expired = false;
  const timer = setTimeout(() => {
    expired = true;
    controller.abort(timeout);
  }, limit.ms);

  try {
    const response = await fetch(url, { ...init, signal: controller.signal });
    if (limit.until === 'headers') clearTimeout(timer);
    return await read(response);
  } catch (error) {
    // Told by the timer, not by the error's shape: a body cut off mid-read
    // surfaces as whatever its reader made of it (`readData` calls it a
    // body that would not decode).
    if (expired && !own?.aborted) throw timeout;
    throw error;
  } finally {
    clearTimeout(timer);
  }
}
