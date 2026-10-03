/**
 * The inspector's Projector row — the GUI face of
 * `gglib model update --projector` / `--no-projector`: the linked file by its
 * name, "None", the daemon's choices, an update on each pick, and the
 * server's refusal in its own words.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { ProjectorRow } from '../../../src/components/ModelInspectorPanel/components/ProjectorRow';
import type { ProjectorChoice } from '../../../src/types/generated/ProjectorChoice';
import { farDetail } from '../fixtures/fakeFarDaemon';

const OWN: ProjectorChoice = { path: '/models/x/X.mmproj-Q8_0.gguf', name: 'X.mmproj-Q8_0.gguf' };
const OTHER: ProjectorChoice = { path: '/models/y/mmproj-F16.gguf', name: 'mmproj-F16.gguf' };

const fetchMock = vi.fn();

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** The daemon answers the choices route with `choices`. */
function offers(choices: ProjectorChoice[]) {
  fetchMock.mockImplementation(async () => json(choices));
}

/** Model 7's detail as this machine sends it, linked to `projectorPath` or to nothing. */
function detail(projectorPath?: string) {
  return farDetail(7, 'X', { projectorPath, imageInput: projectorPath !== undefined });
}

function row(props: Partial<Parameters<typeof ProjectorRow>[0]> = {}) {
  const onUpdateModel = vi.fn().mockResolvedValue(undefined);
  const onChanged = vi.fn();
  render(
    <dl>
      <ProjectorRow modelId={7} detail={detail()} onUpdateModel={onUpdateModel} onChanged={onChanged} {...props} />
    </dl>,
  );
  return { onUpdateModel, onChanged };
}

const picker = () => screen.getByRole('combobox', { name: 'Projector' });
const labels = () => within(picker()).getAllByRole('option').map((o) => o.textContent);

describe('ProjectorRow', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("offers None and the daemon's choices for this model, with None picked on a model that has no projector", async () => {
    offers([OWN, OTHER]);
    row();

    await waitFor(() => expect(labels()).toEqual(['None (text only)', OWN.name, OTHER.name]));
    expect(fetchMock.mock.calls[0][0]).toBe('/api/models/7/projectors');
    expect(picker()).toHaveValue('');
  });

  it("shows the linked projector by its file name, with its path as the picker's title", async () => {
    offers([OWN, OTHER]);
    row({ detail: detail(OTHER.path) });

    await waitFor(() => expect(labels()).toHaveLength(3));
    expect(picker()).toHaveValue(OTHER.path);
    expect(within(picker()).getByRole('option', { name: OTHER.name, selected: true })).toBeInTheDocument();
    expect(picker()).toHaveAttribute('title', OTHER.path);
  });

  it('shows the linked projector by its file name even when the choices do not list it', async () => {
    offers([OWN]);
    row({ detail: detail('/elsewhere/mmproj-other.gguf') });

    await waitFor(() => expect(labels()).toEqual(['None (text only)', 'mmproj-other.gguf', OWN.name]));
    expect(picker()).toHaveValue('/elsewhere/mmproj-other.gguf');
  });

  it('updates the model with the chosen path and tells the owner', async () => {
    offers([OWN, OTHER]);
    const { onUpdateModel, onChanged } = row();
    await waitFor(() => expect(labels()).toHaveLength(3));

    await userEvent.setup().selectOptions(picker(), OTHER.name);

    expect(onUpdateModel).toHaveBeenCalledExactlyOnceWith(7, { projectorPath: OTHER.path });
    await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
  });

  it('updates the model with null when None is chosen', async () => {
    offers([OWN]);
    const { onUpdateModel, onChanged } = row({ detail: detail(OWN.path) });
    await waitFor(() => expect(labels()).toHaveLength(2));

    await userEvent.setup().selectOptions(picker(), 'None (text only)');

    expect(onUpdateModel).toHaveBeenCalledExactlyOnceWith(7, { projectorPath: null });
    await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
  });

  it("shows the server's refusal, keeps the link as it was, and does not tell the owner", async () => {
    offers([OWN, OTHER]);
    const refusal = '/models/y/mmproj-F16.gguf holds model weights, not a projector';
    const onUpdateModel = vi.fn().mockRejectedValue(new Error(refusal));
    const { onChanged } = row({ onUpdateModel, detail: detail(OWN.path) });
    await waitFor(() => expect(labels()).toHaveLength(3));

    await userEvent.setup().selectOptions(picker(), OTHER.name);

    expect(await screen.findByRole('alert')).toHaveTextContent(refusal);
    expect(picker()).toHaveValue(OWN.path);
    expect(picker()).toBeEnabled();
    expect(onChanged).not.toHaveBeenCalled();
  });

  it('clears a refusal when the next pick lands', async () => {
    offers([OWN, OTHER]);
    const onUpdateModel = vi.fn().mockRejectedValueOnce(new Error('refused')).mockResolvedValue(undefined);
    row({ onUpdateModel });
    await waitFor(() => expect(labels()).toHaveLength(3));
    const user = userEvent.setup();

    await user.selectOptions(picker(), OTHER.name);
    expect(await screen.findByRole('alert')).toHaveTextContent('refused');
    await user.selectOptions(picker(), OWN.name);

    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  });

  it("takes no pick while the detail shown is still another model's", async () => {
    offers([OWN]);
    row({ detail: farDetail(6, 'last', { projectorPath: OWN.path, imageInput: true }) });

    await waitFor(() => expect(labels()).toHaveLength(2));
    expect(picker()).toBeDisabled();
    expect(picker()).toHaveValue('');
  });

  it('says how to link one from a terminal when there is nothing to pick', async () => {
    offers([]);
    row();

    expect(await screen.findByText('gglib model update 7 --projector <path>')).toBeInTheDocument();
    expect(labels()).toEqual(['None (text only)']);
  });

  it('does not show the terminal hint when there is a choice', async () => {
    offers([OWN]);
    row();

    await waitFor(() => expect(labels()).toHaveLength(2));
    expect(screen.queryByText(/gglib model update/)).not.toBeInTheDocument();
  });

  it('says so when the choices cannot be read', async () => {
    fetchMock.mockImplementation(async () => json({ error: 'model 7 not found', status: 404 }, 404));
    row();

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Could not read the projector files: model 7 not found',
    );
  });

  it("reads the next model's choices and drops the last model's refusal", async () => {
    fetchMock.mockImplementation(async (url: string) => json(url === '/api/models/8/projectors' ? [OTHER] : [OWN]));
    const onUpdateModel = vi.fn().mockRejectedValue(new Error('refused'));
    const view = render(
      <dl>
        <ProjectorRow modelId={7} detail={detail()} onUpdateModel={onUpdateModel} onChanged={vi.fn()} />
      </dl>,
    );
    await waitFor(() => expect(labels()).toHaveLength(2));
    await userEvent.setup().selectOptions(picker(), OWN.name);
    await screen.findByRole('alert');

    view.rerender(
      <dl>
        <ProjectorRow modelId={8} detail={farDetail(8, 'Y')} onUpdateModel={onUpdateModel} onChanged={vi.fn()} />
      </dl>,
    );

    await waitFor(() => expect(labels()).toEqual(['None (text only)', OTHER.name]));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
