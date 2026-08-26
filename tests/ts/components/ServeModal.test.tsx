/**
 * The serve modal's Context Length box, and the one thing about it that
 * `contextPlaceholder`'s own tests cannot see: that the modal actually asks.
 *
 * The ladder was extracted to a helper so its answers could be asserted
 * instead of eyeballed, and its answers are — but a placeholder written back
 * inline as a string literal would pass every one of those tests, because
 * they call the helper directly. That is how `Model max: N` got there in the
 * first place. See ADR 0009.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { ServeModal } from '../../../src/components/ModelInspectorPanel/components/ServeModal';
import { contextPlaceholder } from '../../../src/components/ModelInspectorPanel/components/contextPlaceholder';
import type { AppSettings, GgufModel } from '../../../src/types';
import { guiModel } from '../fixtures/model';
import { appSettings } from '../fixtures/settings';

function renderModal(model: GgufModel, settings: AppSettings | null) {
  render(
    <ServeModal
      model={model}
      settings={settings}
      customContext=""
      customPort=""
      jinjaOverride={null}
      isServing={false}
      hasAgentTag={false}
      hasMtpTag={false}
      mtpNMaxOverride={null}
      mtpPMinOverride={null}
      inferenceParams={undefined}
      pinProxy={false}
      onContextChange={vi.fn()}
      onPortChange={vi.fn()}
      onJinjaChange={vi.fn()}
      onJinjaReset={vi.fn()}
      onMtpNMaxChange={vi.fn()}
      onMtpPMinChange={vi.fn()}
      onInferenceParamsChange={vi.fn()}
      onPinProxyChange={vi.fn()}
      onClose={vi.fn()}
      onStart={vi.fn()}
    />,
  );
  return screen.getByLabelText(/Context Length/i);
}

describe('ServeModal — the Context Length placeholder', () => {
  it('renders what the ladder says, not a literal of its own', () => {
    // Asserted against the helper rather than a hardcoded string, so this
    // stays true when the ladder's wording changes and fails when the modal
    // stops asking it.
    const model = guiModel({ contextLength: 262144, serverDefaults: { contextLength: 16384 } });
    const settings = appSettings({ defaultContextSize: 8192 });

    expect(renderModal(model, settings)).toHaveAttribute(
      'placeholder',
      contextPlaceholder(model, settings),
    );
  });

  it('tracks the ladder into the unconfigured case', () => {
    // The two states must not collapse to one string, or the assertion above
    // would hold against a constant.
    const bare = guiModel({ contextLength: 262144 });
    const configured = guiModel({ contextLength: 262144, serverDefaults: { contextLength: 16384 } });

    expect(renderModal(bare, null)).toHaveAttribute(
      'placeholder',
      contextPlaceholder(bare, null),
    );
    expect(contextPlaceholder(bare, null)).not.toBe(contextPlaceholder(configured, null));
  });

  it('passes the settings through, not just the model', () => {
    // `contextPlaceholder(model, null)` — dropping the second argument —
    // survived every other assertion here, because no rendered state reached
    // the global-default branch: one test supplies `serverDefaults`, which
    // short-circuits first, and the rest pass `null`. `settings` otherwise
    // survives in this component only for `llamaBasePort`, so a refactor that
    // stopped threading it would tell every user with a stored default that
    // the server decides, when their own number is what gets used.
    const model = guiModel({ contextLength: 262144 });
    const settings = appSettings({ defaultContextSize: 8192 });

    expect(renderModal(model, settings)).toHaveAttribute('placeholder', 'Default: 8,192');
  });

  it('never offers the trained window, whatever the model records', () => {
    // The regression itself: this box used to read `Model max: 262,144`.
    const model = guiModel({ contextLength: 262144 });
    expect(renderModal(model, null).getAttribute('placeholder')).not.toContain('262,144');
  });
});
