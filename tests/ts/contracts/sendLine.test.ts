/**
 * The line above a chat's composer is the one Rust words.
 * `gglib_core::domain::send_line` writes it, and its tests record it for
 * every absence (none, `ModelAbsence::ALL`) and every endpoint state (a
 * pinned one pinned to the chat's model and to another), beside the model
 * it names, in `contracts/vocabulary/send_line.json`; the page's
 * builder must make the same line, or none, of each.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect } from 'vitest';

import { sendLine } from '../../../src/components/status/sendLine';
import type { EndpointState } from '../../../src/types/generated/EndpointState';
import type { ModelAbsence } from '../../../src/types/generated/ModelAbsence';

import { rust } from './rustSource';

interface Recorded {
  model: string;
  absence: ModelAbsence | null;
  endpoint: EndpointState;
  pinned: string | null;
  line: string | null;
}

const RECORDED = JSON.parse(rust('contracts/vocabulary/send_line.json')) as Recorded[];

describe('the send line is the one Rust records', () => {
  it('records every absence on every endpoint state', () => {
    expect(new Set(RECORDED.map((r) => r.absence))).toEqual(new Set([null, 'not_loaded', 'crashed']));
    expect(RECORDED.filter((r) => r.absence === null).map((r) => r.endpoint)).toEqual(['on', 'off', 'pinned', 'pinned', 'starting']);
    expect(RECORDED.filter((r) => r.endpoint === 'pinned').map((r) => r.pinned)).toEqual(Array(3).fill(['Qwen3-4B', 'Qwen3-8B']).flat());
  });

  it('says a send loads a model that is not loaded, and turns an endpoint that is off on', () => {
    const line = (absence: ModelAbsence | null, endpoint: EndpointState, pinned: string | null = null) =>
      RECORDED.find((r) => r.absence === absence && r.endpoint === endpoint && r.pinned === pinned)?.line;
    expect(line('not_loaded', 'on')).toBe("Qwen3-4B isn't loaded · sending loads it");
    expect(line('crashed', 'on')).toBe('Qwen3-4B stopped unexpectedly · sending loads it again');
    expect(line(null, 'off')).toBe('The endpoint is Off · sending turns it on');
    expect(line(null, 'on')).toBeNull();
    expect(line('not_loaded', 'pinned', 'Qwen3-8B')).toBe('The endpoint is Pinned to Qwen3-8B and cannot serve Qwen3-4B · sending is refused');
    expect(line('not_loaded', 'pinned', 'Qwen3-4B')).toBe("Qwen3-4B isn't loaded · sending loads it");
  });

  it.each(RECORDED.map((r) => [`${r.absence ?? 'resident'} / ${r.endpoint}${r.pinned ? ` to ${r.pinned}` : ''}`, r] as const))('%s', (_case, recorded) => {
    expect(sendLine(recorded.model, recorded.absence, recorded.endpoint, recorded.pinned)).toBe(recorded.line);
  });
});
