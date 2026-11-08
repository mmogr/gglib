/**
 * The line above a chat's composer is the one Rust words.
 * `gglib_core::domain::send_line` writes it, and its tests record it for
 * every absence (none, `ModelAbsence::ALL`) and every endpoint state, beside
 * the model it names, in `contracts/vocabulary/send_line.json`; the page's
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
  line: string | null;
}

const RECORDED = JSON.parse(rust('contracts/vocabulary/send_line.json')) as Recorded[];

describe('the send line is the one Rust records', () => {
  it('records every absence on every endpoint state', () => {
    expect(new Set(RECORDED.map((r) => r.absence))).toEqual(new Set([null, 'not_loaded', 'crashed']));
    expect(RECORDED.filter((r) => r.absence === null).map((r) => r.endpoint)).toEqual(['on', 'off', 'pinned', 'starting']);
  });

  it('says a send loads a model that is not loaded, and turns an endpoint that is off on', () => {
    const line = (absence: ModelAbsence | null, endpoint: EndpointState) =>
      RECORDED.find((r) => r.absence === absence && r.endpoint === endpoint)?.line;
    expect(line('not_loaded', 'on')).toBe("Qwen3-4B isn't loaded · sending loads it");
    expect(line('crashed', 'on')).toBe('Qwen3-4B stopped unexpectedly · sending loads it again');
    expect(line(null, 'off')).toBe('The endpoint is Off · sending turns it on');
    expect(line(null, 'on')).toBeNull();
  });

  it.each(RECORDED.map((r) => [`${r.absence ?? 'resident'} / ${r.endpoint}`, r] as const))('%s', (_case, recorded) => {
    expect(sendLine(recorded.model, recorded.absence, recorded.endpoint)).toBe(recorded.line);
  });
});
