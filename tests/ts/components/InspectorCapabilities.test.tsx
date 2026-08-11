/**
 * Tests for the capabilities editor — the GUI face of
 * `gglib model capabilities`. Bits render as checkboxes; each toggle
 * PATCHes exactly one flag and reports back to the owner.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { InspectorCapabilities } from '../../../src/components/ModelInspectorPanel/components/InspectorCapabilities';
import { setModelCapabilities } from '../../../src/services/transport/api/models/local';

vi.mock('../../../src/services/transport/api/models/local', () => ({
  setModelCapabilities: vi.fn().mockResolvedValue({}),
}));

const mockSet = vi.mocked(setModelCapabilities);

describe('InspectorCapabilities', () => {
  beforeEach(() => {
    mockSet.mockClear();
  });

  it('renders the bitfield as checkbox state', () => {
    // supportsSystemRole (1) + supportsToolCalls (4)
    render(
      <InspectorCapabilities modelId={7} capabilities={0b0101} onChanged={vi.fn()} onError={vi.fn()} />,
    );

    expect(screen.getByLabelText(/Supports system role/)).toBeChecked();
    expect(screen.getByLabelText(/Requires strict turns/)).not.toBeChecked();
    expect(screen.getByLabelText(/Supports tool calls/)).toBeChecked();
    expect(screen.getByLabelText(/Supports reasoning/)).not.toBeChecked();
  });

  it('names pass-through mode when every flag is unset', () => {
    render(
      <InspectorCapabilities modelId={7} capabilities={0} onChanged={vi.fn()} onError={vi.fn()} />,
    );
    expect(screen.getByText(/messages pass through untouched/)).toBeInTheDocument();
  });

  // The trap this warns about is real: a non-empty bitfield makes every
  // unticked box mean "no", so ticking one flag silently enforces three.
  it('warns that ticking one flag starts enforcing the rest', () => {
    render(
      <InspectorCapabilities modelId={7} capabilities={0} onChanged={vi.fn()} onError={vi.fn()} />,
    );
    expect(screen.getByText(/Ticking any one box/)).toBeInTheDocument();
  });

  it('says unticked means no once any flag is set', () => {
    render(
      <InspectorCapabilities modelId={7} capabilities={0b1000} onChanged={vi.fn()} onError={vi.fn()} />,
    );
    expect(screen.getByText(/every unticked box is applied as/)).toBeInTheDocument();
    expect(screen.queryByText(/messages pass through untouched/)).not.toBeInTheDocument();
  });

  it('patches exactly the toggled flag and reports the change', async () => {
    const onChanged = vi.fn();
    render(
      <InspectorCapabilities modelId={7} capabilities={0} onChanged={onChanged} onError={vi.fn()} />,
    );

    await userEvent.click(screen.getByLabelText(/Supports reasoning/));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(mockSet).toHaveBeenCalledWith(7, { supportsReasoning: true });
  });

  it('routes failures to the owner instead of swallowing them', async () => {
    mockSet.mockRejectedValueOnce(new Error('capabilities update failed'));
    const onError = vi.fn();
    render(
      <InspectorCapabilities modelId={7} capabilities={0} onChanged={vi.fn()} onError={onError} />,
    );

    await userEvent.click(screen.getByLabelText(/Supports tool calls/));

    await waitFor(() => expect(onError).toHaveBeenCalledWith('capabilities update failed'));
  });
});
