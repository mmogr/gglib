/**
 * The images a reply's tool calls made, and which carry a prompt: only the
 * drawing tool's, on a saved reply, which is what the picture sheet fixes
 * and redraws from.
 */

import { describe, it, expect } from 'vitest';
import { replyImages } from '../../../src/types/messages';

describe('replyImages', () => {
  const image = { id: 'c'.repeat(64), mime: 'image/png', width: 64, height: 64 };
  const call = (toolName: string, args: unknown) => ({ type: 'tool-call', toolName, args, artifact: { images: [image] } });
  const saved = { custom: { dbId: 12 } };

  it('gives a saved reply\'s drawn picture its prompt, and no other tool\'s image one', () => {
    const content = [
      { type: 'text', text: 'Here.' },
      call('builtin:generate_image', { prompt: 'a red fox' }),
      call('builtin:read_file', { prompt: 'a red fox' }),
      call('builtin:generate_image', { prompt: '  ' }),
      call('builtin:generate_image', null),
      { type: 'tool-call', toolName: 'builtin:generate_image', args: { prompt: 'none drawn' } },
    ];
    expect(replyImages({ content, metadata: saved })).toEqual([{ image, prompt: 'a red fox' }, { image }, { image }, { image }]);
  });

  it('gives none a prompt on a reply that is not a saved row: one arriving, or drawn and not read back', () => {
    const content = [call('builtin:generate_image', { prompt: 'a red fox' })];
    expect(replyImages({ content })).toEqual([{ image }]);
    expect(replyImages({ content, metadata: { custom: {} } })).toEqual([{ image }]);
    expect(replyImages({ content: 'plain words', metadata: saved })).toEqual([]);
  });
});
