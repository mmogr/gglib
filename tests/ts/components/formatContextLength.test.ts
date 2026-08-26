/**
 * The inspector's Context Length row is a statement about the model, and it
 * used to label the GGUF's trained window `(default)` — the same claim the
 * Serve action used to act on, rendered one component over. Nothing serves
 * the trained window by default; with nothing configured the server sizes the
 * launch itself. See ADR 0009, and `contextPlaceholder`, which is where
 * "what will a serve actually use" is answered.
 */

import { describe, it, expect } from 'vitest';
import { formatContextLength } from '../../../src/components/ModelInspectorPanel/components/ModelMetadataGrid';
import { guiModel } from '../fixtures/model';

describe('formatContextLength', () => {
  it('labels the GGUF window as trained, not as a default', () => {
    const shown = formatContextLength(guiModel({ contextLength: 262144 }));
    expect(shown).toBe('262,144 (trained)');
    expect(shown).not.toContain('default');
  });

  it('shows a per-model override bare, because that one really is what gets served', () => {
    const model = guiModel({ contextLength: 262144, serverDefaults: { contextLength: 16384 } });
    expect(formatContextLength(model)).toBe('16,384');
  });

  it('says the metadata is absent rather than claiming a default', () => {
    expect(formatContextLength(guiModel({ contextLength: null }))).toBe('Not recorded');
  });
});
