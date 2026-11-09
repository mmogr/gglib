/**
 * A turn's wait, as the daemon logs it, against the page's
 * `AgentWaitingEvent`.
 *
 * `contracts/runs/recorded.json`'s `model_load` and `image_render` are
 * written by `gglib-core`'s runs wire tests from the real
 * `AgentEvent::Waiting` serialisation: a load a second in, and a wait behind
 * a render second in line. Each carries `said`, the sentence Rust words it
 * as (`waiting_sentence`), which the page shows verbatim.
 */

import { describe, it, expect } from 'vitest';

import type { AgentWaitingEvent } from '../../../src/types/events/agentEvent';
import { rust } from './rustSource';

const RECORDED = JSON.parse(rust('contracts/runs/recorded.json')) as {
  model_load: AgentWaitingEvent;
  image_render: AgentWaitingEvent;
};

describe('a wait', () => {
  it('for a model to load carries the keys the page reads, and its words', () => {
    const wait = RECORDED.model_load;
    expect(Object.keys(wait).sort()).toEqual(['elapsed_ms', 'model', 'position', 'reason', 'said', 'step', 'total', 'type']);
    expect(wait.type).toBe('waiting');
    expect(wait.reason).toBe('model_load');
    expect(wait.model).toBe('Qwen3-4B');
    expect(wait.elapsed_ms).toBe(1000);
    expect(wait.said).toBe('Loading Qwen3-4B… 1 s');
  });

  it('behind an image render carries its step, its place in line, and its words', () => {
    const wait = RECORDED.image_render;
    expect(Object.keys(wait).sort()).toEqual(['position', 'reason', 'said', 'step', 'total', 'type']);
    expect(wait.reason).toBe('image_render');
    expect(wait.said).toBe('Waiting for an image render, step 3 of 20, 2 in line');
  });
});
