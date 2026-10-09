/**
 * The fake daemon's branching rules, held to gglib's own.
 *
 * `contracts/chats/branching.json` is written by `gglib-core`'s
 * `branching/contract_tests.rs` from what the Rust rules answer each case
 * with, and fails there when it goes stale; ggchat replays the same file.
 * Here every case, a change's plan and a family's branch points, must be
 * answered the same by `fakeBranches.ts`, which the page's tests run
 * against, so no test of the page passes on a rule the daemon does not hold.
 */

import { describe, it, expect } from 'vitest';

import type { BranchPoint } from '../../../src/types/generated/BranchPoint';
import type { ChatChange } from '../../../src/types/generated/ChatChange';
import { answerable, plan, points, type LineChat, type PathRow, type Planned } from '../fixtures/fakeBranches';
import { rust } from './rustSource';

interface PlanCase {
  name: string;
  path: PathRow[];
  busy: boolean;
  change: ChatChange;
  answer: Planned;
  answerable: boolean;
}

interface PointsCase {
  name: string;
  me: number;
  family: LineChat[];
  points: BranchPoint[];
}

const RECORDED = JSON.parse(rust('contracts/chats/branching.json')) as { plans: PlanCase[]; points: PointsCase[] };

describe('the branching rules the fake daemon holds', () => {
  it('has cases to replay', () => {
    expect(RECORDED.plans.length).toBeGreaterThan(0);
    expect(RECORDED.points.length).toBeGreaterThan(0);
  });

  it.each(RECORDED.plans.map((c) => [c.name, c] as const))('%s', (_name, recorded) => {
    expect(plan(recorded.path, recorded.change, recorded.busy)).toEqual(recorded.answer);
    expect(answerable(recorded.path)).toBe(recorded.answerable);
  });

  it.each(RECORDED.points.map((c) => [c.name, c] as const))('%s', (_name, recorded) => {
    expect(points(recorded.me, recorded.family)).toEqual(recorded.points);
  });
});
