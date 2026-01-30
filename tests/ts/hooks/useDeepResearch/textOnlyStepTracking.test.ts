/**
 * Tests for text-only step tracking in deep research loop.
 * 
 * Verifies that the loop terminates appropriately when the LLM
 * outputs text without calling any tools (potential infinite loop scenario).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  createInitialState,
  ResearchState,
} from '../../../../src/hooks/useDeepResearch/types';
import {
  MAX_TEXT_ONLY_STEPS,
  MAX_LOOP_ITERATIONS,
  CONSECUTIVE_UNPRODUCTIVE_LIMIT,
} from '../../../../src/hooks/useDeepResearch/runResearchLoop';
import {
  buildTurnMessages,
} from '../../../../src/hooks/useDeepResearch/buildTurnMessages';

describe('Text-Only Step Tracking', () => {
  describe('State initialization', () => {
    it('initializes consecutiveTextOnlySteps to 0', () => {
      const state = createInitialState('test query', 'msg-1');
      expect(state.consecutiveTextOnlySteps).toBe(0);
    });

    it('initializes loopIterations to 0', () => {
      const state = createInitialState('test query', 'msg-1');
      expect(state.loopIterations).toBe(0);
    });
  });

  describe('Constants', () => {
    it('MAX_TEXT_ONLY_STEPS should be 3', () => {
      expect(MAX_TEXT_ONLY_STEPS).toBe(3);
    });

    it('MAX_LOOP_ITERATIONS should be 100', () => {
      expect(MAX_LOOP_ITERATIONS).toBe(100);
    });

    it('CONSECUTIVE_UNPRODUCTIVE_LIMIT should be 5', () => {
      expect(CONSECUTIVE_UNPRODUCTIVE_LIMIT).toBe(5);
    });
  });

  describe('Dynamic warning injection', () => {
    it('does not inject warning when consecutiveTextOnlySteps is 0', () => {
      const state = createInitialState('test query', 'msg-1');
      state.phase = 'gathering';
      state.consecutiveTextOnlySteps = 0;
      
      const result = buildTurnMessages({
        state,
        baseSystemPrompt: 'You are a research assistant.',
      });
      
      // Should have exactly 2 messages (system + user)
      expect(result.messages.length).toBe(2);
    });

    it('injects gentle observation on first text-only step', () => {
      const state = createInitialState('test query', 'msg-1');
      state.phase = 'gathering';
      state.consecutiveTextOnlySteps = 1;
      
      const result = buildTurnMessages({
        state,
        baseSystemPrompt: 'You are a research assistant.',
      });
      
      // Should have 3 messages (system + user + warning)
      expect(result.messages.length).toBe(3);
      expect(result.messages[2].role).toBe('system');
      expect(result.messages[2].content).toContain('[SYSTEM OBSERVATION]');
      expect(result.messages[2].content).toContain('did not execute a tool');
    });

    it('injects warning on second text-only step', () => {
      const state = createInitialState('test query', 'msg-1');
      state.phase = 'gathering';
      state.consecutiveTextOnlySteps = 2;
      
      const result = buildTurnMessages({
        state,
        baseSystemPrompt: 'You are a research assistant.',
      });
      
      expect(result.messages.length).toBe(3);
      expect(result.messages[2].content).toContain('[SYSTEM WARNING]');
      expect(result.messages[2].content).toContain('STALLING');
    });

    it('injects critical alert on third+ text-only step', () => {
      const state = createInitialState('test query', 'msg-1');
      state.phase = 'gathering';
      state.consecutiveTextOnlySteps = 3;
      
      const result = buildTurnMessages({
        state,
        baseSystemPrompt: 'You are a research assistant.',
      });
      
      expect(result.messages.length).toBe(3);
      expect(result.messages[2].content).toContain('[CRITICAL ALERT]');
      expect(result.messages[2].content).toContain('IMMINENT TIMEOUT');
    });

    it('does not inject warning in non-gathering phases', () => {
      const state = createInitialState('test query', 'msg-1');
      state.phase = 'planning';
      state.consecutiveTextOnlySteps = 3;
      
      const result = buildTurnMessages({
        state,
        baseSystemPrompt: 'You are a research assistant.',
      });
      
      // Should have exactly 2 messages even with high text-only count
      expect(result.messages.length).toBe(2);
    });
  });

  describe('Step tracking behavior expectations', () => {
    it('text-only counter should trigger unproductive step after threshold', () => {
      // This documents the expected behavior:
      // After MAX_TEXT_ONLY_STEPS (3) consecutive text-only responses,
      // the step counter advances and consecutiveUnproductiveSteps increments
      
      const state = createInitialState('test query', 'msg-1');
      state.phase = 'gathering';
      state.consecutiveTextOnlySteps = MAX_TEXT_ONLY_STEPS;
      
      // When MAX_TEXT_ONLY_STEPS is reached:
      // - Step should advance
      // - consecutiveUnproductiveSteps should increment
      // - consecutiveTextOnlySteps should reset to 0
      
      // After CONSECUTIVE_UNPRODUCTIVE_LIMIT such incidents,
      // the current question should be marked as blocked
      
      const totalIterationsToBlock = MAX_TEXT_ONLY_STEPS * CONSECUTIVE_UNPRODUCTIVE_LIMIT;
      expect(totalIterationsToBlock).toBe(15); // 3 * 5 = 15 text-only responses to block a question
    });

    it('MAX_LOOP_ITERATIONS provides absolute safety backstop', () => {
      // Even if all other logic fails, MAX_LOOP_ITERATIONS (100) ensures the loop stops
      expect(MAX_LOOP_ITERATIONS).toBeGreaterThan(
        MAX_TEXT_ONLY_STEPS * CONSECUTIVE_UNPRODUCTIVE_LIMIT * 3 // Worst case: 3 questions
      );
    });
  });
});
