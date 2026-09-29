/**
 * What the chat routes send, read as they send it.
 *
 * `POST /api/conversations` and `DELETE /api/messages/{id}` answer with a
 * bare JSON number (`Json<i64>` in `crates/gglib-axum/src/chat_api.rs`).
 * The page read `.id`
 * and `.deletedCount` off them, so every id it recorded was `undefined` and
 * an edit of a message saved in the same session deleted nothing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { createConversation, deleteMessage } from '../../../../src/services/transport/api/chat';

const fetchMock = vi.fn();

/** A route's answer as axum writes `Json(n)`: the number, and nothing else. */
function bare(n: number): Response {
  return new Response(JSON.stringify(n), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

describe('chat routes that answer with a bare number', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });

  it('creating a conversation returns its id', async () => {
    fetchMock.mockResolvedValueOnce(bare(41));
    await expect(createConversation({ title: 'New Chat' })).resolves.toBe(41);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/conversations');
  });

  it('deleting a saved message sends its id and returns how many went', async () => {
    fetchMock.mockResolvedValueOnce(bare(3));
    await expect(deleteMessage(12)).resolves.toBe(3);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/messages/12');
    expect(init.method).toBe('DELETE');
  });
});
