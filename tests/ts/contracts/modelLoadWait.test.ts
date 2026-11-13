/**
 * A turn's wait, as the daemon logs it, against the page's
 * `AgentWaitingEvent`.
 *
 * `contracts/runs/recorded.json`'s `model_load` is written by
 * `gglib-core`'s runs wire tests from the real `AgentEvent::Waiting`
 * serialisation: a load a second in. It carries `said`, the sentence Rust
 * words it as (`waiting_sentence`), which the page shows verbatim. Its
 * `agent_slot` is a paired device's run waiting for an agent loop slot.
 */

import { describe, it, expect } from 'vitest';

import type { AgentWaitingEvent } from '../../../src/types/events/agentEvent';
import { rust } from './rustSource';

const RECORDED = JSON.parse(rust('contracts/runs/recorded.json')) as {
  model_load: AgentWaitingEvent;
  agent_slot: AgentWaitingEvent;
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

  it('for an agent slot names no model, says its place in line, and its words', () => {
    const wait = RECORDED.agent_slot;
    expect(Object.keys(wait).sort()).toEqual(['elapsed_ms', 'position', 'reason', 'said', 'step', 'total', 'type']);
    expect(wait.reason).toBe('agent_slot');
    expect(wait.position).toBe(1);
    expect(wait.said).toBe('Waiting for a free agent slot, 1 in line… 2 s');
  });
});
