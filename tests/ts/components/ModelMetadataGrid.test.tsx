/**
 * The inspector's Quantization row: the label a model's type is shown by,
 * then the bits per weight its tensor data measured, where each is known.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import {
  ModelMetadataGrid,
  type MetadataModel,
} from '../../../src/components/ModelInspectorPanel/components/ModelMetadataGrid';

function model(fields: Partial<MetadataModel>): MetadataModel {
  return { paramCountB: 27, ...fields };
}

describe('ModelMetadataGrid quantization row', () => {
  it('shows the label and the bits per weight together', () => {
    render(<ModelMetadataGrid model={model({ quantization: 'Q4_K_P', bitsPerWeight: 5.25 })} />);
    expect(screen.getByText('Q4_K_P · 5.25 bpw')).toBeInTheDocument();
  });

  it('shows the label alone when the bits were not measured', () => {
    render(<ModelMetadataGrid model={model({ quantization: 'Q8_0' })} />);
    expect(screen.getByText('Q8_0')).toBeInTheDocument();
    expect(screen.queryByText(/bpw/)).not.toBeInTheDocument();
  });

  it('shows no row when neither is known', () => {
    render(<ModelMetadataGrid model={model({})} />);
    expect(screen.queryByText('Quantization')).not.toBeInTheDocument();
  });
});
