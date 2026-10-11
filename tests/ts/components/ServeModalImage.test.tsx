/**
 * The serve modal for an image model.
 *
 * An image model has no context window, chat template, MTP or sampling, so
 * the modal offers none of them; it says what loading means instead: the
 * family, each component (a missing one named), and whether the image runtime
 * is installed, with the way to Settings when it is not. Start waits for both.
 * A chat model's modal is unchanged, which the control case pins.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/platform', async (original) => ({
  ...(await original<typeof import('../../../src/services/platform')>()),
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { ServeModal } from '../../../src/components/ModelInspectorPanel/components/ServeModal';
import { resetClientCache, setApiSession } from '../../../src/services/transport/api/client';
import type { ComponentLinkDto, GgufModel } from '../../../src/types';
import { guiModel } from '../fixtures/model';

const flux = (overrides: Partial<GgufModel> = {}) =>
  guiModel({ name: 'flux-schnell', imageFamily: 'flux1', missingComponents: [], ...overrides });

const LINKED: ComponentLinkDto[] = [
  { role: 'vae', path: '/models/ae.safetensors', present: true },
  { role: 'clip_l', path: '/models/clip_l.safetensors', present: true },
  { role: 't5xxl', path: '/models/t5xxl_fp16.safetensors', present: false },
];

const json = (body: unknown) =>
  new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

/** What `GET /api/config/system/sd-status` answers. */
function sdStatus(installed: boolean) {
  return {
    install: {
      installed,
      binaryPath: '/data/.sd/bin/sd-server',
      configPath: '/data/.sd/sd-config.json',
      pinnedRelease: 'master-948-228c707',
      installType: null,
      release: installed ? 'master-948-228c707' : null,
      platform: null,
      installedAt: null,
      recordError: null,
      versionLine: null,
      commit: null,
    },
    prebuilt: 'macOS universal (Metal)',
    prebuiltUnavailable: null,
    warning: null,
    runningModel: null,
    installCommand: 'gglib config sd install',
  };
}

function renderModal(model: GgufModel, onOpenSystemSettings?: () => void) {
  const onStart = vi.fn();
  render(
    <ServeModal
      model={model}
      settings={null}
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
      onStart={onStart}
      components={LINKED}
      onOpenSystemSettings={onOpenSystemSettings}
    />,
  );
  return { onStart };
}

describe('the serve modal for an image model', () => {
  let installed: boolean;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    installed = true;
    fetchMock = vi.fn(async (url: string) =>
      url.endsWith('/api/config/system/sd-status')
        ? json(sdStatus(installed))
        : new Response('{}', { status: 404, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const startButton = () => screen.getByRole('button', { name: 'Start Server' });

  it('offers no context, template, MTP or sampling, and names the family and each component', async () => {
    renderModal(flux());

    expect(await screen.findByText(/Loads on stable-diffusion\.cpp master-948-228c707/)).toBeInTheDocument();
    expect(screen.queryByLabelText(/Context Length/i)).not.toBeInTheDocument();
    expect(screen.queryByText('MTP Speculative Decoding')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Inference Parameters/ })).not.toBeInTheDocument();
    expect(screen.getByText('Flux.1')).toBeInTheDocument();
    expect(screen.getByText('ae.safetensors')).toBeInTheDocument();
    expect(screen.getByText('t5xxl_fp16.safetensors (file missing)')).toBeInTheDocument();
    expect(startButton()).toBeEnabled();
  });

  it('names the missing components and will not start', async () => {
    const { onStart } = renderModal(flux({ missingComponents: ['vae', 't5xxl'] }));

    expect(await screen.findByText(/Missing VAE, T5-XXL/)).toBeInTheDocument();
    expect(startButton()).toBeDisabled();
    await userEvent.click(startButton());
    expect(onStart).not.toHaveBeenCalled();
  });

  it('says the runtime is not installed, links to Settings, and will not start', async () => {
    installed = false;
    const onOpenSystemSettings = vi.fn();
    renderModal(flux(), onOpenSystemSettings);

    expect(await screen.findByText(/The image runtime is not installed/)).toBeInTheDocument();
    expect(screen.getByText('gglib config sd install')).toBeInTheDocument();
    expect(startButton()).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: 'Open Settings' }));
    expect(onOpenSystemSettings).toHaveBeenCalledTimes(1);
  });

  it('leaves a chat model\'s modal as it was, and never asks for the image runtime', async () => {
    renderModal(guiModel({ name: 'qwen' }));

    expect(screen.getByLabelText(/Context Length/i)).toBeInTheDocument();
    expect(screen.getByText('MTP Speculative Decoding')).toBeInTheDocument();
    expect(startButton()).toBeEnabled();
    // The modal's other read, the sampling explanation, goes out for every
    // model; once it has, an image runtime read would have gone out too.
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
    await act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
    const urls = (fetchMock.mock.calls as [string][]).map(([url]) => url);
    expect(urls.some((url) => url.endsWith('/sd-status')), urls.join(', ')).toBe(false);
  });
});
