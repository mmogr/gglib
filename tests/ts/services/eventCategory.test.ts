/**
 * The wire-tag → category mapping every `/api/events` frame passes through.
 *
 * A tag with no arm here resolves to `null`, and `validateEvent` drops the
 * frame without a word — so an unmapped family is not a degraded feature, it
 * is a silent one. The model events spent their whole existence in that state:
 * the variants were declared, `event_name()` mapped them, and the frontend
 * threw every one away.
 */

import { describe, it, expect } from 'vitest';

import { getEventCategory } from '../../../src/services/transport/events/category';

describe('getEventCategory', () => {
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
   * `model` is a prefix of nothing else on the wire, but the guard is cheap
   * and the `server_`/`server_snapshot` pair shows how easily a prefix arm
   * swallows a sibling family.
   */
  it('does not claim tags that merely start with the same letters', () => {
    expect(getEventCategory('modelling_something')).toBeNull();
  });
});
