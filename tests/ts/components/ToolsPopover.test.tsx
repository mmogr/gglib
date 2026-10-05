/**
 * Which way the Tools popout grows.
 *
 * It lined up with the button's right edge and grew leftwards, which suited
 * a button at the right of a header. The notebook moved the button into the
 * composer's left margin, where growing leftwards put the popout off the
 * page; the composer's case is in `ChatPageNotebook.test.tsx`. This is the
 * default, which must still grow leftwards.
 *
 * And what the popout asks about the chat's model: whether its template
 * reads a reasoning effort, once, the first time it opens, and never for a
 * chat with no model of this machine.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const getModelDetail = vi.hoisted(() => vi.fn());
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => ({ getModelDetail }) };
});

import { ToolsPopover } from '../../../src/components/ToolsPopover';

const NOTE = /template does not declare reasoning effort/;
const NO_MODEL = 'Applies to models whose template declares reasoning effort; others ignore it.';
const effortField = () => screen.queryByRole('combobox', { name: 'Reasoning Effort' });

beforeEach(() => {
  getModelDetail.mockReset();
  getModelDetail.mockResolvedValue({ reasoningEffortSupport: 'no' });
});

describe('ToolsPopover', () => {
  it('lines up with the right edge by default, and caps its height', async () => {
    render(<ToolsPopover />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Tools' }));

    const popout = screen.getByText(/active$/).closest('.z-popover');
    expect(popout).toHaveClass('right-0');
    expect(popout).not.toHaveClass('left-0');
    expect(popout).toHaveClass('max-h-[70vh]', 'overflow-y-auto');
  });

  it('asks nothing about a model when it has none, and says where the effort level applies', async () => {
    render(<ToolsPopover />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Tools' }));

    expect(effortField()).toHaveAccessibleDescription(NO_MODEL);
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
    expect(getModelDetail).not.toHaveBeenCalled();
  });

  it("asks for its model's template support the first time it opens, not before and not again", async () => {
    const user = userEvent.setup();
    render(<ToolsPopover modelId={7} />);
    expect(getModelDetail).not.toHaveBeenCalled();

    const tools = screen.getByRole('button', { name: 'Tools' });
    await user.click(tools);
    expect(getModelDetail).toHaveBeenCalledTimes(1);
    expect(getModelDetail).toHaveBeenCalledWith(7);
    expect(await screen.findByText(NOTE)).toBeInTheDocument();
    expect(effortField()).not.toBeInTheDocument();
    expect(screen.getByLabelText('Reasoning budget')).toBeInTheDocument();

    await user.click(tools);
    await user.click(tools);
    // Known from the first asking: the note is there at once.
    expect(screen.getByText(NOTE)).toBeInTheDocument();
    expect(getModelDetail).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['unknown', 'Not yet observed — start the model to find out whether its template reads this. Until then a level set here is sent as given.'],
    ['yes', "This model's template reads reasoning effort, so a level set here is honoured."],
  ])('keeps the dropdown where the answer is %s', async (support, caption) => {
    getModelDetail.mockResolvedValue({ reasoningEffortSupport: support });
    render(<ToolsPopover modelId={7} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Tools' }));

    await waitFor(() => expect(effortField()).toHaveAccessibleDescription(caption));
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });

  it.each([
    ['fails', () => getModelDetail.mockRejectedValue(new Error('the catalogue did not answer'))],
    ['finds no such model', () => getModelDetail.mockResolvedValue(null)],
  ])('keeps the dropdown, as for no model, when the asking %s', async (_, answer) => {
    answer();
    render(<ToolsPopover modelId={7} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Tools' }));
    await waitFor(() => expect(getModelDetail).toHaveBeenCalledTimes(1));
    await getModelDetail.mock.results[0].value.catch(() => {});

    expect(effortField()).toHaveAccessibleDescription(NO_MODEL);
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });

  it("never says one model's answer of another: a chat that moves off this machine's model says where the level applies", async () => {
    const view = render(<ToolsPopover modelId={7} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Tools' }));
    expect(await screen.findByText(NOTE)).toBeInTheDocument();

    view.rerender(<ToolsPopover />);
    expect(effortField()).toHaveAccessibleDescription(NO_MODEL);
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });
});
