import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { PathText } from '../../../src/components/primitives';

/** The text between the places a line may break, as the element holds it. */
function runs(path: string): string[] {
  const { container } = render(<PathText path={path} />);
  const code = container.querySelector('code') as HTMLElement;
  expect(code).toHaveTextContent(path, { normalizeWhitespace: false });
  return code.innerHTML.split('<wbr>');
}

describe('PathText', () => {
  it('lets a Windows path wrap after each backslash and nowhere inside a name', () => {
    expect(runs('C:\\Users\\me\\AppData\\Local\\llama_models')).toEqual([
      'C:\\',
      'Users\\',
      'me\\',
      'AppData\\',
      'Local\\',
      'llama_models',
    ]);
  });

  it('lets a Unix path wrap after each slash', () => {
    expect(runs('/Users/me/.local/share/llama_models')).toEqual(['/', 'Users/', 'me/', '.local/', 'share/', 'llama_models']);
  });

  it('keeps a run of separators together, and a path that ends in one', () => {
    expect(runs('\\\\server\\share\\')).toEqual(['\\\\', 'server\\', 'share\\']);
  });

  it('holds a name with no separator whole', () => {
    expect(runs('models')).toEqual(['models']);
  });

  it('breaks inside a name only when the name outruns the line', () => {
    const { container } = render(<PathText path="C:\models" className="text-text" />);
    expect(container.querySelector('code')).toHaveClass('[overflow-wrap:anywhere]', 'text-text');
  });
});
