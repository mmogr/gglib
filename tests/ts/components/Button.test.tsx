/**
 * A text link has no box of its own: its size sets only its text size, so a
 * link that wraps grows with its lines instead of spilling out of a fixed
 * height onto the line below (#1838). Every other button keeps its height
 * and padding.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { Button, type ButtonSize } from '../../../src/components/ui/Button';

const classesOf = (name: string) => screen.getByRole('button', { name }).className.split(/\s+/);

describe('Button', () => {
  it.each<[ButtonSize, string]>([
    ['sm', 'text-xs'],
    ['md', 'text-sm'],
    ['lg', 'text-sm'],
  ])('a %s link takes only its text size, with no height or padding', (size, text) => {
    render(
      <Button variant="link" size={size}>
        huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF
      </Button>,
    );
    const classes = classesOf('huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF');
    expect(classes).toContain(text);
    expect(classes.filter((c) => /^(h|p|px|py)-/.test(c))).toEqual([]);
    // Its wrapped lines start where the text does, not centred.
    expect(classes).toEqual(expect.arrayContaining(['justify-start', 'text-left']));
  });

  it('a link still takes the classes its caller gives it', () => {
    render(
      <Button variant="link" size="sm" className="font-mono font-normal">
        Retry
      </Button>,
    );
    expect(classesOf('Retry')).toEqual(expect.arrayContaining(['font-mono', 'font-normal', 'text-xs']));
  });

  it('every other button keeps its height and padding', () => {
    render(
      <>
        <Button variant="ghost" size="sm">
          Not now
        </Button>
        <Button variant="secondary">Chat</Button>
      </>,
    );
    expect(classesOf('Not now')).toEqual(expect.arrayContaining(['h-7', 'px-2.5', 'justify-center']));
    expect(classesOf('Chat')).toEqual(expect.arrayContaining(['h-8', 'px-3']));
  });
});
