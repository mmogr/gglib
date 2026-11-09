/**
 * The line above a chat's composer is the one Rust words.
 * `gglib_core::domain::send_line` writes it, and its tests record it for
 * every absence (none, `ModelAbsence::ALL`) and every endpoint state (a
 * pinned one pinned to the chat's model, to another, and to another of the
 * same name, since a pin is matched by id), beside the model it names, in
 * `contracts/vocabulary/send_line.json`; the page's
 * builder must make the same line, or none, of each.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect } from 'vitest';

import { sendLine, type NamedModel } from '../../../src/components/status/sendLine';
import type { EndpointState } from '../../../src/types/generated/EndpointState';
import type { ModelAbsence } from '../../../src/types/generated/ModelAbsence';

import { rust } from './rustSource';

interface Recorded {
  model: NamedModel;
  absence: ModelAbsence | null;
  endpoint: EndpointState;
  pinned: NamedModel | null;
  line: string | null;
}

const RECORDED = JSON.parse(rust('contracts/vocabulary/send_line.json')) as Recorded[];

describe('the send line is the one Rust records', () => {
  it('records every absence on every endpoint state', () => {
    expect(new Set(RECORDED.map((r) => r.absence))).toEqual(new Set([null, 'not_loaded', 'crashed']));
    expect(RECORDED.filter((r) => r.absence === null).map((r) => r.endpoint)).toEqual(['on', 'off', 'pinned', 'pinned', 'pinned', 'starting']);
    expect(new Set(RECORDED.map((r) => JSON.stringify(r.model)))).toEqual(new Set([JSON.stringify({ id: 4, name: 'Qwen3-4B' })]));
    const pins = [{ id: 4, name: 'Qwen3-4B' }, { id: 8, name: 'Qwen3-8B' }, { id: 5, name: 'Qwen3-4B' }];
    expect(RECORDED.filter((r) => r.endpoint === 'pinned').map((r) => r.pinned)).toEqual(Array(3).fill(pins).flat());
  });

  it('says a send loads a model that is not loaded, and turns an endpoint that is off on', () => {
    const line = (absence: ModelAbsence | null, endpoint: EndpointState, pinnedId: number | null = null) =>
      RECORDED.find((r) => r.absence === absence && r.endpoint === endpoint && (r.pinned?.id ?? null) === pinnedId)?.line;
    expect(line('not_loaded', 'on')).toBe("Qwen3-4B isn't loaded · sending loads it");
    expect(line('crashed', 'on')).toBe('Qwen3-4B stopped unexpectedly · sending loads it again');
    expect(line(null, 'off')).toBe('The endpoint is Off · sending turns it on');
    expect(line(null, 'on')).toBeNull();
    expect(line('not_loaded', 'pinned', 8)).toBe('The endpoint is Pinned to Qwen3-8B and cannot serve Qwen3-4B · sending is refused');
    expect(line('not_loaded', 'pinned', 4)).toBe("Qwen3-4B isn't loaded · sending loads it");
    // Pinned to another model of the same name: another id, so refused.
    expect(line('not_loaded', 'pinned', 5)).toBe('The endpoint is Pinned to Qwen3-4B and cannot serve Qwen3-4B · sending is refused');
  });

  it.each(RECORDED.map((r) => [`${r.absence ?? 'resident'} / ${r.endpoint}${r.pinned ? ` to ${r.pinned.name} (${r.pinned.id})` : ''}`, r] as const))('%s', (_case, recorded) => {
    expect(sendLine(recorded.model, recorded.absence, recorded.endpoint, recorded.pinned)).toBe(recorded.line);
  });
});
