import { describe, it } from 'vitest';

// WaveScrubber component is not yet implemented.
// Tests are marked todo until src/pages/Council/components/WaveScrubber.tsx exists.
describe('WaveScrubber', () => {
  it.todo('renders nothing when there are fewer than 2 wave boundaries');
  it.todo('renders nothing for empty event list');
  it.todo('renders wave scrubber section with 2+ waves');
  it.todo('renders a badge for each completed wave');
  it.todo('shows confirmation dialog when a wave badge is clicked');
  it.todo('calls onRewind with the correct wave index on confirm');
  it.todo('dismisses dialog on cancel without calling onRewind');
  it.todo('does not open dialog when disabled');
});

