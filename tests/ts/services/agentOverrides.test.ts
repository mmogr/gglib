/**
 * The client-wide overrides store the Agent limits sheet writes: what it
 * keeps, it reads back. No turn carries it (D6 moves it to saved settings).
 */

import { describe, it, expect, beforeEach } from 'vitest';

import {
  writeStoredAgentOverrides,
  readStoredAgentOverrides,
} from '../../../src/services/agentOverrides';

describe('the overrides store', () => {
  beforeEach(() => localStorage.clear());

  it('round-trips both halves through storage', () => {
    writeStoredAgentOverrides({ reasoningEffort: 'max', reasoningBudgetTokens: -1 });

    expect(readStoredAgentOverrides()).toEqual({
      reasoningEffort: 'max',
      reasoningBudgetTokens: -1,
    });
  });
});
