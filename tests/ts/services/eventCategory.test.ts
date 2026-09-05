/**
 * The wire-tag → category mapping every `/api/events` frame passes through.
 *
 * A tag with no arm here resolves to `null`, and `validateEvent` drops the
 * frame without a word — so an unmapped family is not a degraded feature, it
 * is a silent one. The model events spent their whole existence in that state:
 * the variants were declared, `event_name()` mapped them, and the frontend
 * threw every one away.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, it, expect } from 'vitest';

import { getEventCategory } from '../../../src/services/transport/events/category';

/**
 * Every `type` tag the generated `AppEvent` can carry.
 *
 * Read out of the binding rather than listed here, so a variant added in Rust
 * arrives in this test on the next `make bindings` instead of whenever
 * somebody remembers. That is the failure this file exists for: the model
 * events were declared in Rust, mapped by `event_name()`, and silently
 * discarded by the frontend for their whole existence.
 */
const WIRE_TAGS: string[] = (() => {
  const binding = readFileSync(
    resolve(import.meta.dirname, '../../../src/types/generated/AppEvent.ts'),
    'utf8',
  );
  const tags = [...binding.matchAll(/"type": "([a-z_]+)"/g)].map((m) => m[1]);
  if (tags.length === 0) throw new Error('found no tags in the AppEvent binding');
  return [...new Set(tags)];
})();

describe('getEventCategory', () => {
  /**
   * The map's five categories against the union's arms. `AppEventMap` claims
   * to cover `AppEvent`, but `getEventCategory` is ordinary runtime code and
   * the compiler cannot check that claim — a new `server_*` arm widens the
   * `Extract` slice with no type error while the router returns `null` for it
   * and `validateEvent` drops the frame in silence.
   */
  it('claims every tag the wire union can carry', () => {
    const unrouted = WIRE_TAGS.filter((tag) => getEventCategory(tag) === null);

    expect(unrouted).toEqual([]);
    expect(WIRE_TAGS.length).toBeGreaterThan(10); // extractor sanity check
  });

  /**
   * The tunnel's five tags arrived with the `RemoteOps` binding and, like the
   * model events before them, would have been dropped in silence: the test
   * above caught them the moment the binding was regenerated.
   */
  it('routes the remote tunnel tags to the remote category', () => {
    for (const tag of [
      'remote_enabled',
      'remote_disabled',
      'remote_paired',
      'remote_connected',
      'remote_disconnected',
    ]) {
      expect(getEventCategory(tag)).toBe('remote');
    }
  });

  it('routes the three model lifecycle tags to the model category', () => {
    expect(getEventCategory('model_added')).toBe('model');
    expect(getEventCategory('model_updated')).toBe('model');
    expect(getEventCategory('model_removed')).toBe('model');
  });

  it('keeps routing the families that already worked', () => {
    expect(getEventCategory('download')).toBe('download');
    expect(getEventCategory('server_started')).toBe('server');
    expect(getEventCategory('server_snapshot')).toBe('server');
    expect(getEventCategory('verification_progress')).toBe('verification');
    expect(getEventCategory('proxy_started')).toBe('proxy');
  });

  it('returns null for a tag no consumer claims', () => {
    expect(getEventCategory('nonsense')).toBeNull();
    expect(getEventCategory('')).toBeNull();
  });

  /**
   * There is no `log` family on this stream and never was.
   *
   * `AppEvent` is the only type `/api/events` carries, and none of its
   * fourteen tags begins with `log`. Server logs are real, but they are a
   * different route — `/api/servers/{port}/logs/stream`, framing bare
   * `ServerLogEntry` objects that carry no `type` at all and never reach this
   * function.
   */
  it('claims no log family, which this stream does not carry', () => {
    expect(getEventCategory('log')).toBeNull();
    expect(getEventCategory('log_line')).toBeNull();
  });

  /**
   * `model` is a prefix of nothing else on the wire, but the guard is cheap
   * and the `server_`/`server_snapshot` pair shows how easily a prefix arm
   * swallows a sibling family.
   */
  it('does not claim tags that merely start with the same letters', () => {
    expect(getEventCategory('modelling_something')).toBeNull();
  });
});
