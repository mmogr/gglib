/**
 * Code in a reply: inline code stays in its line, a block is one block.
 */

import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import MarkdownMessageContent from '../../../src/components/ChatMessagesPanel/components/MarkdownMessageContent';

describe('MarkdownMessageContent', () => {
  it('draws inline code inside its paragraph, not as a block', () => {
    const { container } = render(<MarkdownMessageContent text={'Set `KeepAlive` to `true`.'} />);
    expect(container.querySelector('pre')).toBeNull();
    expect(container.querySelectorAll('p code')).toHaveLength(2);
  });

  it('draws a fenced block as one block, highlighted', () => {
    const { container } = render(<MarkdownMessageContent text={'```xml\n<dict/>\n```'} />);
    expect(container.querySelectorAll('pre')).toHaveLength(1);
    expect(container.querySelector('pre > code')?.className).toMatch(/language-xml/);
  });
});
