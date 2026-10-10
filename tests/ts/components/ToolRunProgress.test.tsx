/**
 * How far a running tool has got, under its row: the words for each stage,
 * with the counts the tool gave and none it did not; a bar that is empty
 * before the steps, filled by them and full after; and the picture so far.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ToolRunProgress, progressWords } from '../../../src/components/ToolExecutionProgress/ToolRunProgress';
import type { ToolProgress } from '../../../src/types/messages';

describe('progressWords', () => {
  it('says each stage, with the counts it was given', () => {
    expect(progressWords({ stage: 'queued', position: 2 })).toBe('Queued, 2 in line');
    expect(progressWords({ stage: 'loading' })).toBe('Loading');
    expect(progressWords({ stage: 'sampling', pass: 1, done: 3, total: 20 })).toBe('Sampling 3 of 20');
    expect(progressWords({ stage: 'decoding' })).toBe('Decoding');
    expect(progressWords({ stage: 'finishing' })).toBe('Finishing');
  });

  it('says no count the tool did not give', () => {
    expect(progressWords({ stage: 'queued' })).toBe('Queued');
    expect(progressWords({ stage: 'queued', position: 0 })).toBe('Queued');
    expect(progressWords({ stage: 'sampling' })).toBe('Sampling');
    expect(progressWords({ stage: 'sampling', done: 3 })).toBe('Sampling');
    expect(progressWords({ stage: 'sampling', done: 0, total: 20 })).toBe('Sampling 0 of 20');
  });

  it('names the image being made from the second on', () => {
    expect(progressWords({ stage: 'sampling', pass: 2, done: 3, total: 20 })).toBe('Sampling 3 of 20, image 2');
    expect(progressWords({ stage: 'sampling', pass: 1, done: 20, total: 20 })).toBe('Sampling 20 of 20');
  });

  it('says a stage it does not know as it came', () => {
    expect(progressWords({ stage: 'upscaling' } as unknown as ToolProgress)).toBe('upscaling');
  });
});

describe('ToolRunProgress', () => {
  const bar = () => screen.getByRole('progressbar');

  it('draws nothing for a tool that has said nothing and made no frame', () => {
    const { container } = render(<ToolRunProgress />);
    expect(container).toBeEmptyDOMElement();
  });

  it('fills the bar by the steps done: none before sampling, all of it after', () => {
    const { rerender } = render(<ToolRunProgress progress={{ stage: 'loading' }} />);
    expect(bar()).toHaveAttribute('aria-valuenow', '0');
    expect(screen.getByText('Loading')).toBeInTheDocument();

    rerender(<ToolRunProgress progress={{ stage: 'sampling', done: 15, total: 20 }} />);
    expect(bar()).toHaveAttribute('aria-valuenow', '75');
    rerender(<ToolRunProgress progress={{ stage: 'sampling' }} />);
    expect(bar()).toHaveAttribute('aria-valuenow', '0');
    rerender(<ToolRunProgress progress={{ stage: 'decoding' }} />);
    expect(bar()).toHaveAttribute('aria-valuenow', '100');
    rerender(<ToolRunProgress progress={{ stage: 'finishing' }} />);
    expect(bar()).toHaveAttribute('aria-valuenow', '100');
  });

  it('shows a frame as an image of its own type, with or without progress beside it', () => {
    render(<ToolRunProgress preview={{ mime: 'image/png', step: 3, total: 20, b64: 'iVBORw0KGgo=' }} />);
    const shown = screen.getByRole('img', { name: 'Preview of the image being made, step 3 of 20' });
    expect(shown).toHaveAttribute('src', 'data:image/png;base64,iVBORw0KGgo=');
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });
});
