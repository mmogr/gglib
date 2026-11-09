/**
 * What this machine's turn carries: the conversation and the new message,
 * and nothing the daemon decides. No history, no port, no model, no tool
 * filter and no limits: the daemon rebuilds the history from what is saved
 * and runs the turn on what the conversation's settings name. The
 * conversation's Thinking choice and its folder are keys only on the turn
 * that changes them; `draw` only on a turn sent with Draw pressed, and
 * `image_model` only beside it.
 */

import { describe, it, expect } from 'vitest';

import { buildTurn, mintRunId, type TurnOptions } from '../../../../src/hooks/useGglibRuntime/runRequest';

/** Options with every knob at rest, so each test states only its own fields. */
function options(extra: Partial<TurnOptions> = {}): TurnOptions {
  return { conversationId: 4, content: 'hello', ...extra };
}

describe('buildTurn', () => {
  it('is the conversation and the message, and nothing else', () => {
    expect(buildTurn(options())).toStrictEqual({ conversation_id: 4, content: 'hello' });
  });

  it('carries the images by their ids, in order, and no key when there are none', () => {
    expect(buildTurn(options({ images: ['b', 'a'] }))).toStrictEqual({ conversation_id: 4, content: 'hello', images: ['b', 'a'] });
    expect(Object.keys(buildTurn(options({ images: [] })))).not.toContain('images');
  });

  it('answers the saved question with no message of its own', () => {
    expect(buildTurn(options({ content: '', answerSaved: true }))).toStrictEqual({
      conversation_id: 4,
      content: '',
      answer_saved: true,
    });
    expect(Object.keys(buildTurn(options({ answerSaved: false })))).not.toContain('answer_saved');
  });

  it('says the Thinking choice only when given one, and changes nothing else by it', () => {
    const unsaid = buildTurn(options());
    expect(buildTurn(options({ thinking: 'off' }))).toStrictEqual({ ...unsaid, thinking: 'off' });
    expect(buildTurn(options({ thinking: 'default' }))).toStrictEqual({ ...unsaid, thinking: 'default' });
  });

  it('says the folder only on a turn that changes it: a path sets it, "" forgets it', () => {
    const unsaid = buildTurn(options());
    expect(Object.keys(buildTurn(options({ folder: undefined })))).not.toContain('folder');
    expect(buildTurn(options({ folder: '/home/m/src' }))).toStrictEqual({ ...unsaid, folder: '/home/m/src' });
    // Clear's "" is sent, not dropped.
    expect(buildTurn(options({ folder: '' }))).toStrictEqual({ ...unsaid, folder: '' });
  });

  it('says draw only for a send made with Draw pressed', () => {
    const unsaid = buildTurn(options());
    expect(Object.keys(buildTurn(options({ draw: false })))).not.toContain('draw');
    expect(buildTurn(options({ draw: true }))).toStrictEqual({ ...unsaid, draw: true });
  });

  it('says the image model only beside draw, by its id', () => {
    const drawn = buildTurn(options({ draw: true }));
    expect(buildTurn(options({ draw: true, imageModel: 7 }))).toStrictEqual({ ...drawn, image_model: 7 });
    // A choice is the chat's, held between drawings: a turn without Draw never carries it.
    for (const draw of [false, undefined]) {
      expect(buildTurn(options({ draw, imageModel: 7 }))).toStrictEqual({ conversation_id: 4, content: 'hello' });
    }
  });
});

describe('mintRunId', () => {
  it('mints ids the hub accepts, and a new one each time', () => {
    const a = mintRunId();
    expect(a).toMatch(/^[A-Za-z0-9_-]{1,64}$/);
    expect(mintRunId()).not.toBe(a);
  });
});
