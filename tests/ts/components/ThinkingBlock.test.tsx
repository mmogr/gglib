/**
 * Code in reasoning: inline code stays in its line, a block is one block.
 */

import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import ThinkingBlock from '../../../src/components/ChatMessagesPanel/components/ThinkingBlock';

const renderThinking = (thinking: string) =>
  render(<ThinkingBlock messageId="m1" segmentIndex={0} thinking={thinking} durationSeconds={null} />);

describe('ThinkingBlock', () => {
  it('draws inline code inside its paragraph, not as a block', () => {
    const { container } = renderThinking('Set `KeepAlive` to `true`.');
    expect(container.querySelector('pre')).toBeNull();
    expect(container.querySelectorAll('p code')).toHaveLength(2);
  });

  it('draws a fenced block as one block, highlighted', () => {
    const { container } = renderThinking('```xml\n<dict/>\n```');
    expect(container.querySelectorAll('pre')).toHaveLength(1);
    expect(container.querySelector('pre > code')?.className).toMatch(/language-xml/);
  });
});
