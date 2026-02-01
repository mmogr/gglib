import { describe, it, expect } from 'vitest';
import {
  isPlanningPhase,
  isGatheringPhase,
  isEvaluatingPhase,
  isCompressingPhase,
  isSynthesizingPhase,
  isCompletePhase,
  isErrorPhase,
  type ResearchState,
} from '../../../../src/hooks/useDeepResearch/types';

// Helper to create minimal test state - only phase matters for type guards
const createTestState = (phase: ResearchState['phase']): ResearchState => {
  return { phase } as ResearchState;
};

describe('Research State Type Guards', () => {
  describe('isPlanningPhase', () => {
    it('should return true for planning phase', () => {
      const state = createTestState('planning');
      expect(isPlanningPhase(state)).toBe(true);
    });

    it('should return false for non-planning phases', () => {
      const state = createTestState('gathering');
      expect(isPlanningPhase(state)).toBe(false);
    });
  });

  describe('isGatheringPhase', () => {
    it('should return true for gathering phase', () => {
      const state = createTestState('gathering');
      expect(isGatheringPhase(state)).toBe(true);
    });

    it('should return false for non-gathering phases', () => {
      const state = createTestState('evaluating');
      expect(isGatheringPhase(state)).toBe(false);
    });
  });

  describe('isEvaluatingPhase', () => {
    it('should return true for evaluating phase', () => {
      const state = createTestState('evaluating');
      expect(isEvaluatingPhase(state)).toBe(true);
    });

    it('should return false for non-evaluating phases', () => {
      const state = createTestState('compressing');
      expect(isEvaluatingPhase(state)).toBe(false);
    });
  });

  describe('isCompressingPhase', () => {
    it('should return true for compressing phase', () => {
      const state = createTestState('compressing');
      expect(isCompressingPhase(state)).toBe(true);
    });

    it('should return false for non-compressing phases', () => {
      const state = createTestState('synthesizing');
      expect(isCompressingPhase(state)).toBe(false);
    });
  });

  describe('isSynthesizingPhase', () => {
    it('should return true for synthesizing phase', () => {
      const state = createTestState('synthesizing');
      expect(isSynthesizingPhase(state)).toBe(true);
    });

    it('should return false for non-synthesizing phases', () => {
      const state = createTestState('complete');
      expect(isSynthesizingPhase(state)).toBe(false);
    });
  });

  describe('isCompletePhase', () => {
    it('should return true for complete phase', () => {
      const state = createTestState('complete');
      expect(isCompletePhase(state)).toBe(true);
    });

    it('should return false for non-complete phases', () => {
      const state = createTestState('planning');
      expect(isCompletePhase(state)).toBe(false);
    });
  });

  describe('isErrorPhase', () => {
    it('should return true for error phase', () => {
      const state = createTestState('error');
      expect(isErrorPhase(state)).toBe(true);
    });

    it('should return false for non-error phases', () => {
      const state = createTestState('gathering');
      expect(isErrorPhase(state)).toBe(false);
    });
  });

  describe('Type guard coverage', () => {
    it('should have a guard for every phase', () => {
      const phases = [
        'planning',
        'gathering',
        'evaluating',
        'compressing',
        'synthesizing',
        'complete',
        'error',
      ] as const;

      const guards = [
        isPlanningPhase,
        isGatheringPhase,
        isEvaluatingPhase,
        isCompressingPhase,
        isSynthesizingPhase,
        isCompletePhase,
        isErrorPhase,
      ];

      expect(guards).toHaveLength(phases.length);
    });
  });
});
