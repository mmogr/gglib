/**
 * A reply just drawn and the same reply loaded say the same of how it was
 * made.
 *
 * `contracts/runs/turn_made.json` is written by `gglib-core`'s replay tests:
 * a turn's frames as the loop logs them, and the metadata the daemon saves
 * from them. The page draws the frames (`dispatchAgentEvent`) and loads the
 * metadata (`buildLoadedMessage`); both must give the same figures, the
 * figure line and its tooltip the same words, the line the same as `gglib
 * chat` prints it (`TurnUsage::figure_line`, recorded beside them), and the
 * composer's ring the same context reading.
 */

import { describe, it, expect, vi } from 'vitest';
import type { GglibMessage } from '../../../src/types/messages';
import type { AgentEvent } from '../../../src/types/events/agentEvent';
import { rust } from './rustSource';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));

import { dispatchAgentEvent } from '../../../src/hooks/useGglibRuntime/agentEventDispatch';
import { buildLoadedMessage } from '../../../src/hooks/useChatPersistence/buildLoadedMessage';
import {
  figureDetail,
  figureParts,
  replyFacts,
} from '../../../src/readings/turnFigures';
import {
  contextLines,
  contextReading,
  decidingMade,
} from '../../../src/readings/contextReading';

const CONTRACT = JSON.parse(rust('contracts/runs/turn_made.json')) as {
  frames: AgentEvent[];
  metadata: Record<string, unknown>;
  figure_line: string;
  one_token: { metadata: Record<string, unknown>; figure_line: string };
};

function drawn(): GglibMessage {
  let messages: GglibMessage[] = [{ id: 'drawn-1', role: 'assistant', content: [] }];
  const setMessages = (u: GglibMessage[] | ((p: GglibMessage[]) => GglibMessage[])) => {
    messages = typeof u === 'function' ? u(messages) : u;
  };
  const state = { currentId: 'drawn-1' };
  const deps = { setMessages, timingTracker: undefined, makeNextMessage: () => 'next', cleanup: () => {} };
  for (const frame of CONTRACT.frames) dispatchAgentEvent(frame, state, deps);
  return messages[0];
}

function loaded(metadata: Record<string, unknown> = CONTRACT.metadata): GglibMessage {
  return buildLoadedMessage(
    { id: 12, conversation_id: 1, role: 'assistant', content: 'It restarts.', created_at: '2026-09-30T09:13:00Z', metadata },
    1,
  ) as GglibMessage;
}

const facts = (m: GglibMessage) =>
  replyFacts({ id: m.id!, content: Array.isArray(m.content) ? m.content : [], metadata: m.metadata });

describe('turn_made contract', () => {
  it('draws and loads the same figures', () => {
    const custom = (m: GglibMessage) => (m.metadata as { custom?: { made?: unknown } }).custom?.made;
    expect(custom(drawn())).toBeDefined();
    expect(custom(drawn())).toEqual(custom(loaded()));
    // Every key the daemon saved is read, the device's name among them.
    expect(Object.keys(custom(loaded()) as object).sort()).toEqual(Object.keys(CONTRACT.metadata).sort());
  });

  it('says the same figures either way, the daemon\'s rate as it sent and saved it, as gglib chat prints them', () => {
    const line = '41 s · 13 t/s · 2,100 tokens reused';
    expect(CONTRACT.figure_line).toBe(line);
    expect(figureParts(facts(drawn())).join(' · ')).toBe(line);
    expect(figureParts(facts(loaded())).join(' · ')).toBe(line);
    const detail = figureDetail(facts(loaded()));
    expect(detail[0]).toMatch(/^Qwen3\.8-27B\b/);
    expect(detail).toContain('3,180 tokens read');
    expect(figureDetail(facts(drawn()))).toEqual(detail);
  });

  it('says one token reused in the singular, as gglib chat prints it', () => {
    const line = '0.5 s · 1 token reused';
    expect(CONTRACT.one_token.figure_line).toBe(line);
    expect(figureParts(facts(loaded(CONTRACT.one_token.metadata))).join(' · ')).toBe(line);
  });

  it('gives the same context reading either way, from the counts and the size the turn carried', () => {
    const reading = (m: GglibMessage) => contextReading(decidingMade([m]));
    expect(reading(drawn())).toEqual({ used: 3676, size: 8192, percent: 45, severity: 'normal', trimmed: 3, cutOff: false });
    expect(reading(loaded())).toEqual(reading(drawn()));
    expect(contextLines(reading(loaded())!)).toEqual([
      '3,676 of 8,192 tokens (45%) after the last finished reply.',
      '3 earlier messages were shortened or left out to fit.',
    ]);
  });
});
