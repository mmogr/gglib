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

import { farDetail } from '../fixtures/fakeFarDaemon';
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

describe('ModelMetadataGrid revision row', () => {
  it('shows a commit sha short, with the whole sha on hover', () => {
    const sha = '0123456789abcdef0123456789abcdef01234567';
    render(
      <ModelMetadataGrid
        model={model({})}
        detail={farDetail(3, 'qwen', { revision: { kind: 'sha', value: sha, short: '0123456789ab' } })}
      />,
    );
    expect(screen.getByText('Commit')).toBeInTheDocument();
    // Exact: the whole sha contains its own prefix, so a substring match
    // would pass with `value` shown.
    expect(screen.getByTitle(sha)).toHaveTextContent(/^0123456789ab$/);
  });

  it('shows a ref whole, never sliced to a sha', () => {
    const ref = 'release/2026-11-candidate';
    render(
      <ModelMetadataGrid
        model={model({})}
        detail={farDetail(3, 'qwen', { revision: { kind: 'ref', value: ref, short: ref } })}
      />,
    );
    expect(screen.getByText('Revision')).toBeInTheDocument();
    expect(screen.getByText(ref)).toBeInTheDocument();
    expect(screen.queryByText('release/')).toBeNull();
  });

  it('shows the short form the daemon chose, whatever the value', () => {
    // Synthetic: the daemon never shortens a ref, so a ref whose `short`
    // differs from its `value` does not occur. The grid is shown one to
    // prove it prints `short` and never works a form out for itself.
    render(
      <ModelMetadataGrid
        model={model({})}
        detail={farDetail(3, 'qwen', { revision: { kind: 'ref', value: 'refs/heads/main', short: 'main' } })}
      />,
    );
    expect(screen.getByTitle('refs/heads/main')).toHaveTextContent(/^main$/);
  });

  it('shows no revision row when the daemon recorded none', () => {
    render(<ModelMetadataGrid model={model({})} detail={farDetail(3, 'qwen')} />);
    expect(screen.queryByText('Commit')).toBeNull();
    expect(screen.queryByText('Revision')).toBeNull();
  });
});
