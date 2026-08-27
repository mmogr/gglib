/**
 * The serve modal's Context Length placeholder is a promise about what
 * leaving the box empty will do, so each answer it gives has to match the rung
 * the daemon's ladder will actually land on
 * (`resolve_context_size_with_source`,
 * `crates/gglib-core/src/server_config.rs`).
 *
 * It has been wrong twice. It used to offer `Model max: N`, which was true
 * only because the Serve action sent the trained window as an explicit
 * override; when that was removed the placeholder promised a fit in two
 * states that never reach the fitted rung. See ADR 0009.
 */

import { describe, it, expect } from 'vitest';
import { contextPlaceholder } from '../../../src/components/ModelInspectorPanel/components/contextPlaceholder';
import { guiModel } from '../fixtures/model';
import { appSettings } from '../fixtures/settings';

describe('contextPlaceholder', () => {
  it('names the per-model default first, because it outranks the stored one', () => {
    // `ModelServerDefaults` sits above `GlobalDefault` on the ladder, and
    // nothing puts the stored default on the explicit rung any more. This
    // expectation read `Default: 8,192` while the Serve action still did.
    const model = guiModel({ contextLength: 262144, serverDefaults: { contextLength: 16384 } });
    expect(contextPlaceholder(model, appSettings({ defaultContextSize: 8192 }))).toBe(
      'Model default: 16,384',
    );
  });

  it('names the stored default when the model has no override of its own', () => {
    const model = guiModel({ contextLength: 262144 });
    expect(contextPlaceholder(model, appSettings({ defaultContextSize: 8192 }))).toBe(
      'Default: 8,192',
    );
  });

  it('never names the trained window, whatever else it says', () => {
    // The claim ADR 0009 opens on. The Serve action used to send this number
    // as an explicit override, and the placeholder used to advertise it.
    const model = guiModel({ contextLength: 262144 });
    expect(contextPlaceholder(model, null)).not.toContain('262,144');
    expect(contextPlaceholder(model, appSettings({ defaultContextSize: 8192 }))).not.toContain(
      '262,144',
    );
  });

  it('does not promise a fit, because it cannot know the fit is reachable', () => {
    // `fit_context` returns `None` unless it can read the device budget, the
    // weight size and the KV geometry. ADR 0009 records that AMD, Intel,
    // Vulkan and CPU-only hosts get no fit at all and fall through to the 4096
    // floor — so "fitted to this machine" would be a false promise to every
    // one of them. The trained window makes no difference to the answer.
    expect(contextPlaceholder(guiModel({ contextLength: 262144 }), null)).toBe(
      'Sized per launch',
    );
    expect(contextPlaceholder(guiModel({ contextLength: null }), null)).toBe(
      'Sized per launch',
    );
  });
});
