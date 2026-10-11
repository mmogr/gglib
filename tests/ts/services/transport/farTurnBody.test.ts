/**
 * What a far chat's turn sends about drawing, read as it sends it.
 *
 * `draw` is in the body only for a turn sent with Draw pressed, and
 * `image_model` only beside it: the choice is the chat's, held between
 * drawings, and a far machine refuses the key without `draw`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { addFarTurn } from '../../../../src/services/transport/api/farChats';

const fetchMock = vi.fn();

/** The body of the one turn sent. */
function sent(): unknown {
  const [url, init] = fetchMock.mock.calls.at(-1) as [string, RequestInit];
  expect(url).toBe('/api/remote/chats/4/turns/chat-1');
  return JSON.parse(String(init.body));
}

describe('a far turn, of drawing', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    fetchMock.mockImplementation(async () => new Response('{}', { status: 201, headers: { 'content-type': 'application/json' } }));
    vi.stubGlobal('fetch', fetchMock);
  });

  it('says the image model by its id beside draw', async () => {
    await addFarTurn(4, 'chat-1', 'a fox', [], undefined, true, 7);
    expect(sent()).toEqual({ content: 'a fox', draw: true, image_model: 7 });
  });

  it('says draw alone when no image model was chosen', async () => {
    await addFarTurn(4, 'chat-1', 'a fox', [], undefined, true);
    expect(sent()).toEqual({ content: 'a fox', draw: true });
  });

  it('never says the image model on a turn sent without Draw', async () => {
    await addFarTurn(4, 'chat-1', 'hello', [], undefined, false, 7);
    expect(sent()).toEqual({ content: 'hello' });
  });
});
