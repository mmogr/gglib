/**
 * The inspector's Components row — the GUI face of
 * `gglib model update --component <role>=<path>` / `--no-component <role>`:
 * one picker per role the model's family needs, each with its linked file by
 * name, "None" and the daemon's choices for that role, an update naming only
 * that role on each pick, and the server's refusal in its own words under
 * the role it was for.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { ComponentsRow } from '../../../src/components/ModelInspectorPanel/components/ComponentsRow';
import type { ComponentChoices, ComponentLinkDto, ComponentRole } from '../../../src/types';
import { farDetail } from '../fixtures/fakeFarDaemon';

const AE = { path: '/models/unsloth/FLUX.1-schnell/ae.safetensors', name: 'ae.safetensors' };
const CLIP = { path: '/models/comfyanonymous/flux_text_encoders/clip_l.safetensors', name: 'clip_l.safetensors' };
const T5 = { path: '/models/comfyanonymous/flux_text_encoders/t5xxl_fp16.safetensors', name: 't5xxl_fp16.safetensors' };

/** The choices a Flux.1 model is offered: one file known for each role, or none. */
function flux(files: Partial<Record<ComponentRole, (typeof AE)[]>> = {}): ComponentChoices[] {
  return (['vae', 'clip_l', 't5xxl'] as const).map((role) => ({ role, files: files[role] ?? [] }));
}

const fetchMock = vi.fn();

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** The daemon answers the choices route with `choices`. */
function offers(choices: ComponentChoices[]) {
  fetchMock.mockImplementation(async () => json(choices));
}

/** Model 7's detail as a Flux.1 model, linked to `components`; every other Flux role is missing. */
function detail(components: ComponentLinkDto[] = [], id = 7) {
  const linked = new Set(components.map((c) => c.role));
  return farDetail(id, 'flux', {
    imageFamily: 'flux1',
    components,
    missingComponents: (['vae', 'clip_l', 't5xxl'] as const).filter((r) => !linked.has(r)),
  });
}

function row(props: Partial<Parameters<typeof ComponentsRow>[0]> = {}) {
  const onUpdateModel = vi.fn().mockResolvedValue(undefined);
  const onChanged = vi.fn();
  const view = render(
    <dl>
      <ComponentsRow modelId={7} detail={detail()} onUpdateModel={onUpdateModel} onChanged={onChanged} {...props} />
    </dl>,
  );
  return { onUpdateModel, onChanged, view };
}

const picker = (name: string) => screen.getByRole('combobox', { name });
const labels = (name: string) => within(picker(name)).getAllByRole('option').map((o) => o.textContent);

describe('ComponentsRow', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("offers one picker per role, in the daemon's order, each with None and that role's choices", async () => {
    offers(flux({ vae: [AE], clip_l: [CLIP], t5xxl: [T5] }));
    row();

    await waitFor(() => expect(labels('VAE')).toEqual(['None', AE.name]));
    expect(fetchMock.mock.calls[0][0]).toBe('/api/models/7/components');
    expect(screen.getAllByRole('combobox').map((c) => c.getAttribute('aria-label'))).toEqual([
      'VAE',
      'CLIP-L',
      'T5-XXL',
    ]);
    expect(labels('CLIP-L')).toEqual(['None', CLIP.name]);
    expect(labels('T5-XXL')).toEqual(['None', T5.name]);
    expect(picker('VAE')).toHaveValue('');
  });

  it('shows a linked file by its name, with its path as the title, and the others at None', async () => {
    offers(flux({ vae: [AE], t5xxl: [T5] }));
    row({ detail: detail([{ role: 'vae', path: AE.path, present: true }]) });

    await waitFor(() => expect(labels('VAE')).toHaveLength(2));
    expect(picker('VAE')).toHaveValue(AE.path);
    expect(picker('VAE')).toHaveAttribute('title', AE.path);
    expect(picker('T5-XXL')).toHaveValue('');
    expect(screen.queryByText('No file is at the linked path.')).not.toBeInTheDocument();
  });

  it('shows a linked file the choices do not list, and says when no file is at its path', async () => {
    offers(flux());
    row({ detail: detail([{ role: 'vae', path: '/gone/ae.safetensors', present: false }]) });

    await waitFor(() => expect(labels('VAE')).toEqual(['None', 'ae.safetensors']));
    expect(picker('VAE')).toHaveValue('/gone/ae.safetensors');
    expect(screen.getByText('No file is at the linked path.')).toBeInTheDocument();
  });

  it('updates the model with only the picked role and its path, and tells the owner', async () => {
    offers(flux({ vae: [AE], t5xxl: [T5] }));
    const { onUpdateModel, onChanged } = row();
    await waitFor(() => expect(labels('T5-XXL')).toHaveLength(2));

    await userEvent.setup().selectOptions(picker('T5-XXL'), T5.name);

    expect(onUpdateModel).toHaveBeenCalledExactlyOnceWith(7, { components: { t5xxl: T5.path } });
    await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
  });

  it('clears a role by sending null for it when None is picked', async () => {
    offers(flux({ vae: [AE] }));
    const { onUpdateModel, onChanged } = row({ detail: detail([{ role: 'vae', path: AE.path, present: true }]) });
    await waitFor(() => expect(labels('VAE')).toHaveLength(2));

    await userEvent.setup().selectOptions(picker('VAE'), 'None');

    expect(onUpdateModel).toHaveBeenCalledExactlyOnceWith(7, { components: { vae: null } });
    await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
  });

  it("shows the server's refusal under its role, keeps the link, and does not tell the owner", async () => {
    offers(flux({ vae: [AE, CLIP] }));
    const refusal = `${CLIP.path} is not a Flux.1 VAE: expected decoder.conv_in.weight with 16 input channels`;
    const onUpdateModel = vi.fn().mockRejectedValue(new Error(refusal));
    const { onChanged } = row({ onUpdateModel, detail: detail([{ role: 'vae', path: AE.path, present: true }]) });
    await waitFor(() => expect(labels('VAE')).toHaveLength(3));

    await userEvent.setup().selectOptions(picker('VAE'), CLIP.name);

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(refusal);
    // Under the VAE picker, not another role's.
    expect(picker('VAE').parentElement?.parentElement).toContainElement(alert);
    expect(picker('CLIP-L').parentElement?.parentElement).not.toContainElement(alert);
    expect(picker('VAE')).toHaveValue(AE.path);
    expect(picker('VAE')).toBeEnabled();
    expect(onChanged).not.toHaveBeenCalled();
  });

  it('clears a refusal when the next pick of that role lands', async () => {
    offers(flux({ vae: [AE, CLIP] }));
    const onUpdateModel = vi.fn().mockRejectedValueOnce(new Error('refused')).mockResolvedValue(undefined);
    row({ onUpdateModel });
    await waitFor(() => expect(labels('VAE')).toHaveLength(3));
    const user = userEvent.setup();

    await user.selectOptions(picker('VAE'), CLIP.name);
    expect(await screen.findByRole('alert')).toHaveTextContent('refused');
    await user.selectOptions(picker('VAE'), AE.name);

    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  });

  it("takes no pick while the detail shown is still another model's", async () => {
    offers(flux({ vae: [AE] }));
    row({ detail: detail([{ role: 'vae', path: AE.path, present: true }], 6) });

    await waitFor(() => expect(labels('VAE')).toHaveLength(2));
    expect(picker('VAE')).toBeDisabled();
    expect(picker('VAE')).toHaveValue('');
  });

  it('says how to link a role from a terminal when nothing is known for it', async () => {
    offers(flux({ vae: [AE] }));
    row();

    expect(await screen.findByText('gglib model update 7 --component clip_l=<path>')).toBeInTheDocument();
    expect(screen.getByText('gglib model update 7 --component t5xxl=<path>')).toBeInTheDocument();
    expect(screen.queryByText('gglib model update 7 --component vae=<path>')).not.toBeInTheDocument();
  });

  it("draws the roles from the detail until the daemon's choices arrive", async () => {
    fetchMock.mockImplementation(() => new Promise(() => {}));
    row({ detail: detail([{ role: 'vae', path: AE.path, present: true }]) });

    expect(screen.getAllByRole('combobox').map((c) => c.getAttribute('aria-label'))).toEqual([
      'VAE',
      'CLIP-L',
      'T5-XXL',
    ]);
    expect(picker('VAE')).toHaveValue(AE.path);
  });

  it('draws nothing for a family that needs no separate file', async () => {
    offers([]);
    const { view } = row({ detail: farDetail(7, 'sdxl', { imageFamily: 'sdxl' }) });

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    expect(view.container.querySelector('dt')).toBeNull();
  });

  it('says so when the choices cannot be read', async () => {
    fetchMock.mockImplementation(async () => json({ error: 'model 7 not found', status: 404 }, 404));
    row();

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Could not read the component files: model 7 not found',
    );
  });

  it("reads the next model's choices and drops the last model's refusal", async () => {
    fetchMock.mockImplementation(async (url: string) =>
      json(url === '/api/models/8/components' ? flux({ vae: [AE] }) : flux({ vae: [CLIP] })),
    );
    const onUpdateModel = vi.fn().mockRejectedValue(new Error('refused'));
    const view = render(
      <dl>
        <ComponentsRow modelId={7} detail={detail()} onUpdateModel={onUpdateModel} onChanged={vi.fn()} />
      </dl>,
    );
    await waitFor(() => expect(labels('VAE')).toHaveLength(2));
    await userEvent.setup().selectOptions(picker('VAE'), CLIP.name);
    await screen.findByRole('alert');

    view.rerender(
      <dl>
        <ComponentsRow modelId={8} detail={detail([], 8)} onUpdateModel={onUpdateModel} onChanged={vi.fn()} />
      </dl>,
    );

    await waitFor(() => expect(labels('VAE')).toEqual(['None', AE.name]));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
