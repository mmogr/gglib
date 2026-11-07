/**
 * A turn's wait for its model to load, as the daemon logs it, against the
 * page's `AgentWaitingEvent`.
 *
 * `contracts/runs/recorded.json`'s `model_load` is written by `gglib-core`'s
 * runs wire tests from the real `AgentEvent::Waiting` serialisation: a load
 * a second in. It names the model by the catalogue's name and says how long
 * the load has taken, which a client that joins part-way shows as
 * "Loading Qwen3-4B… 1 s".
 */

import { describe, it, expect } from 'vitest';

import type { AgentWaitingEvent } from '../../../src/types/events/agentEvent';
import { rust } from './rustSource';

const RECORDED = JSON.parse(rust('contracts/runs/recorded.json')) as { model_load: AgentWaitingEvent };

describe('a model load wait', () => {
  it('carries the keys the page reads, the model and the time taken among them', () => {
    const wait = RECORDED.model_load;
    expect(Object.keys(wait).sort()).toEqual(['elapsed_ms', 'model', 'position', 'reason', 'step', 'total', 'type']);
    expect(wait.type).toBe('waiting');
    expect(wait.reason).toBe('model_load');
    expect(wait.model).toBe('Qwen3-4B');
    expect(wait.elapsed_ms).toBe(1000);
  });
});
