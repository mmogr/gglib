/**
 * What the agent limits sheet (the composer's Tools ▸ › Agent limits…) asks
 * about the chat's model: whether its template reads a reasoning effort,
 * once, the first time it opens, and never for a chat with no model of this
 * machine. Opening it from the composer is `ChatPlaceComposer.test.tsx`'s.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

const getModelDetail = vi.hoisted(() => vi.fn());
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => ({ getModelDetail }) };
});

import { AgentLimitsSheet } from '../../../src/components/ChatMessagesPanel/components/AgentLimitsSheet';

const NOTE = /template does not declare reasoning effort/;
const NO_MODEL = 'Applies to models whose template declares reasoning effort; others ignore it.';
const effortField = () => screen.queryByRole('combobox', { name: 'Reasoning Effort' });
const sheet = (open: boolean, modelId?: number) => <AgentLimitsSheet open={open} onClose={() => {}} modelId={modelId} />;

beforeEach(() => {
  getModelDetail.mockReset();
  getModelDetail.mockResolvedValue({ reasoningEffortSupport: 'no' });
});

describe('AgentLimitsSheet', () => {
  it('asks nothing about a model when it has none, and says where the effort level applies', () => {
    render(sheet(true));
    expect(screen.getByRole('dialog', { name: 'Agent limits' })).toBeInTheDocument();
    expect(effortField()).toHaveAccessibleDescription(NO_MODEL);
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
    expect(getModelDetail).not.toHaveBeenCalled();
  });

  it("asks for its model's template support the first time it opens, not before and not again", async () => {
    const view = render(sheet(false, 7));
    expect(getModelDetail).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    view.rerender(sheet(true, 7));
    expect(getModelDetail).toHaveBeenCalledTimes(1);
    expect(getModelDetail).toHaveBeenCalledWith(7);
    expect(await screen.findByText(NOTE)).toBeInTheDocument();
    expect(effortField()).not.toBeInTheDocument();
    expect(screen.getByLabelText('Reasoning budget')).toBeInTheDocument();

    view.rerender(sheet(false, 7));
    view.rerender(sheet(true, 7));
    // Known from the first asking: the note is there at once.
    expect(screen.getByText(NOTE)).toBeInTheDocument();
    expect(getModelDetail).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['unknown', 'Not yet observed — start the model to find out whether its template reads this. Until then a level set here is sent as given.'],
    ['yes', "This model's template reads reasoning effort, so a level set here is honoured."],
  ])('keeps the dropdown where the answer is %s', async (support, caption) => {
    getModelDetail.mockResolvedValue({ reasoningEffortSupport: support });
    render(sheet(true, 7));
    await waitFor(() => expect(effortField()).toHaveAccessibleDescription(caption));
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });

  it.each([
    ['fails', () => getModelDetail.mockRejectedValue(new Error('the catalogue did not answer'))],
    ['finds no such model', () => getModelDetail.mockResolvedValue(null)],
  ])('keeps the dropdown, as for no model, when the asking %s', async (_, answer) => {
    answer();
    render(sheet(true, 7));
    await waitFor(() => expect(getModelDetail).toHaveBeenCalledTimes(1));
    await getModelDetail.mock.results[0].value.catch(() => {});

    expect(effortField()).toHaveAccessibleDescription(NO_MODEL);
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });

  it("never says one model's answer of another: a chat that moves off this machine's model says where the level applies", async () => {
    const view = render(sheet(true, 7));
    expect(await screen.findByText(NOTE)).toBeInTheDocument();

    view.rerender(sheet(true));
    expect(effortField()).toHaveAccessibleDescription(NO_MODEL);
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });
});
