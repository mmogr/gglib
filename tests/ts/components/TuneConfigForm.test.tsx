/**
 * The tune form must not send `weights`.
 *
 * It has no weights UI, so anything it sent could only be a hand-copied
 * duplicate of the server's `ScoreWeights::default()` — and the day that
 * default changes, a copy silently overrides it. `TuneConfig.weights` is
 * `#[serde(default)]` server-side, so omitting the key lets each server apply
 * its own, which is also what keeps a newer GUI working against an older
 * daemon that still expects a `speed` member.
 *
 * Without this test the property is held up only by a type that *permits*
 * omission rather than requiring it, so re-adding a literal would type-check
 * and pass — which is the exact drift removing it was meant to prevent.
 *
 * The Rust client has its own copy of this guarantee, asserted on the config
 * types themselves in `gglib_core::domain::benchmark`. It needs one for a
 * different reason: there, `None` serialises as `null` unless told otherwise,
 * and `null` is not the same as absent to the older daemon.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import { TuneConfigForm } from '../../../src/components/Benchmark/Tune/TuneConfigForm';
import { guiModel } from '../fixtures/model';

const model = guiModel({
  name: 'Qwen3-8B',
  filePath: '/models/Qwen3-8B-Q4_K_M.gguf',
  paramCountB: 8,
  addedAt: '2026-01-01T00:00:00Z',
});

describe('TuneConfigForm', () => {
  it('omits weights from the submitted config', () => {
    const onSubmit = vi.fn();
    render(<TuneConfigForm models={[model]} disabled={false} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByRole('button', { name: /run tune/i }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    const [config] = onSubmit.mock.calls[0];

    // `not.toHaveProperty` rather than a check on the serialised body: an
    // explicit `weights: undefined` would serialise away under
    // JSON.stringify and pass, while still being a hand-maintained copy
    // waiting to be filled in.
    expect(config).not.toHaveProperty('weights');
  });
});
