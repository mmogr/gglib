/**
 * The paired machine's model in the inspector: its detail with no path on
 * that machine's disk, Chat and Load only (and only as that machine's actions
 * list them), "Serving on" in place of Load, the actions disabled while its
 * rows are not current, a re-read once a Load lands, and a Load's wait and
 * failure shown only on the model it was pressed for.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
  openUrl: vi.fn(),
}));

import { FarModelInspector } from '../../../src/components/ModelInspectorPanel/FarModelInspector';
import type { PairedModelsState, PairedReach } from '../../../src/hooks/usePairedModels';
import type { ModelAction } from '../../../src/types/generated/ModelAction';
import type { ModelRef } from '../../../src/types/generated/ModelRef';
import { FAR_FINGERPRINT, FakeFarDaemon, farDetail, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';

const model: ModelRef = { machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 3 };

let daemons: FakeFarDaemon;

beforeEach(() => {
  daemons = new FakeFarDaemon();
  daemons.models[3] = farDetail(3, 'qwen3', { hfRepoId: 'Qwen/Qwen3-8B-GGUF' });
  vi.stubGlobal('fetch', vi.fn(daemons.fetch));
});
afterEach(() => {
  vi.unstubAllGlobals();
});

function paired(
  reach: PairedReach = 'reached',
  actions?: ModelAction[],
  capabilities?: string[],
): PairedModelsState {
  const group = pairedModels([farEntry('qwen3', 3, { capabilities }), farEntry('llava', 4, { capabilities: ['vision'] })]);
  return { group: actions ? { ...group, actions } : group, name: 'desk', reach, refetch: vi.fn() };
}

describe('FarModelInspector', () => {
  it('shows the detail and the command, offers Chat and Load, and nothing that changes the model', async () => {
    const onChat = vi.fn();
    render(<FarModelInspector model={model} paired={paired()} onChat={onChat} />);

    expect(await screen.findByRole('heading', { name: 'qwen3' })).toBeInTheDocument();
    expect(screen.getByText('Qwen/Qwen3-8B-GGUF')).toBeInTheDocument();
    // That machine's disk is not this one's to show.
    expect(screen.queryByText('Path')).not.toBeInTheDocument();
    expect(screen.getByText('gglib chat 3 --remote')).toBeInTheDocument();
    // The one other control opens the model's page on Hugging Face.
    const buttons = screen.getAllByRole('button').map((b) => b.getAttribute('aria-label') ?? b.textContent);
    expect(buttons).toEqual(['Open on HuggingFace', 'Chat', 'Load on desk']);

    await userEvent.setup().click(screen.getByRole('button', { name: 'Chat' }));
    expect(onChat).toHaveBeenCalledWith(model, 'qwen3');
  });

  it('marks a model that reads images, without a path or a picker for its projector', async () => {
    // What that machine lists decides it, as for the row: the detail here says otherwise.
    daemons.models[3] = farDetail(3, 'qwen3', { imageInput: false });
    render(<FarModelInspector model={model} paired={paired('reached', undefined, ['vision'])} onChat={vi.fn()} />);

    await screen.findByRole('heading', { name: 'qwen3' });
    expect(screen.getByText('Vision')).toBeInTheDocument();
    expect(screen.queryByText('Projector')).not.toBeInTheDocument();
  });

  it.each([[undefined], [['embeddings']]])(
    'does not mark a model its machine lists with capabilities %j, whatever another model or its detail has',
    async (capabilities) => {
      daemons.models[3] = farDetail(3, 'qwen3', { imageInput: true });
      render(<FarModelInspector model={model} paired={paired('reached', undefined, capabilities)} onChat={vi.fn()} />);

      await screen.findByRole('heading', { name: 'qwen3' });
      expect(screen.queryByText('Vision')).not.toBeInTheDocument();
    },
  );

  it('Load loads it there, reads it again, and then says it is serving and offers no Load', async () => {
    const state = paired();
    render(<FarModelInspector model={model} paired={state} onChat={vi.fn()} />);

    await userEvent.setup().click(await screen.findByRole('button', { name: 'Load on desk' }));

    expect(await screen.findByText('Serving on desk.')).toBeInTheDocument();
    expect(daemons.farCount('POST', '/api/remote/models/3/load')).toBe(1);
    expect(daemons.farCount('GET', '/api/remote/models/3')).toBe(2);
    expect(state.refetch).toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: /load/i })).not.toBeInTheDocument();
  });

  it("shows another model picked during a Load neither that Load's wait nor its failure", async () => {
    daemons.models[4] = farDetail(4, 'llama');
    let fail: () => void = () => {};
    const held = new Promise<Response>((resolve) => {
      fail = () => resolve(new Response(JSON.stringify({ error: 'out of memory', status: 503 }), { status: 503 }));
    });
    vi.stubGlobal('fetch', vi.fn((input: string | URL | Request, init?: RequestInit) =>
      String(input).endsWith('/load') ? held : daemons.fetch(input, init)));
    const view = render(<FarModelInspector model={model} paired={paired()} onChat={vi.fn()} />);
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Load on desk' }));
    expect(screen.getByRole('button', { name: 'Loading…' })).toBeDisabled();

    view.rerender(<FarModelInspector model={{ ...model, id: 4 }} paired={paired()} onChat={vi.fn()} />);
    await screen.findByRole('heading', { name: 'llama' });
    expect(screen.getByRole('button', { name: 'Load on desk' })).toBeEnabled();

    await act(async () => fail());
    expect(screen.queryByText(/Could not load it/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Load on desk' })).toBeEnabled();
  });

  it('offers only the actions that machine lists', async () => {
    render(<FarModelInspector model={model} paired={paired('reached', ['list', 'detail'])} onChat={vi.fn()} />);
    await screen.findByRole('heading', { name: 'qwen3' });
    expect(screen.queryByRole('button', { name: /chat|load/i })).not.toBeInTheDocument();
  });

  it.each(['stale', 'away'] as const)('disables its actions while the rows are %s', async (reach) => {
    render(<FarModelInspector model={model} paired={paired(reach)} onChat={vi.fn()} />);
    await screen.findByRole('heading', { name: 'qwen3' });
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Load on desk' })).toBeDisabled();
  });

  it('says why when that machine does not have it', async () => {
    render(<FarModelInspector model={{ ...model, id: 9 }} paired={paired()} onChat={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/Could not read it from desk/));
    expect(screen.getByRole('button', { name: 'Chat' })).toBeDisabled();
  });
});
