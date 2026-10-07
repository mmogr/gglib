/**
 * A chat's title, as the page asks the daemon for it (`POST /api/chat`).
 *
 * The body is typed by the binding generated from the daemon's
 * `ChatTitleRequest`, and the daemon refuses any key that type does not
 * name, so a cap under another spelling does not compile here and is not
 * dropped there. What the type cannot say is the values, so this reads the
 * JSON itself, key for key. The reply is the model's text, and a refusal is
 * shown in the title's own sentence.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { DEFAULT_TITLE_GENERATION_PROMPT, generateChatTitle } from '../../../../src/services/transport/api/chat';
import { TransportError } from '../../../../src/services/transport/errors';

const fetchMock = vi.fn();

const row = (role: 'user' | 'assistant', content: string) => ({
  id: 1, conversation_id: 1, role, content, created_at: '2026-10-07T00:00:00Z',
});

const chat = [row('user', 'What is a tabby?'), row('assistant', '<think>cats</think>A striped cat.')];

/** The daemon's answer as axum writes `Json(text)`: the string, and nothing else. */
function answer(text: string): Response {
  return new Response(JSON.stringify(text), { status: 200, headers: { 'content-type': 'application/json' } });
}

/** A refusal as the daemon writes one: its status line, and in the body its reason and its code when it has one. */
function refusal(status: number, statusText: string, error: string, type?: string): Response {
  return new Response(JSON.stringify({ error, status, type }), {
    status,
    statusText,
    headers: { 'content-type': 'application/json' },
  });
}

describe('a chat title request', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('sends its port, the chat and then the instruction, its temperature and its cap, and nothing else', async () => {
    fetchMock.mockResolvedValueOnce(answer('Tabby cats'));

    await generateChatTitle({ serverPort: 9000, messages: chat });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(path).toBe('/api/chat');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toStrictEqual({
      port: 9000,
      messages: [
        { role: 'user', content: 'What is a tabby?' },
        { role: 'assistant', content: 'A striped cat.' },
        { role: 'user', content: DEFAULT_TITLE_GENERATION_PROMPT },
      ],
      temperature: 0.7,
      max_tokens: 20,
    });
  });

  it('ends the chat with the instruction it was given, when it was given one', async () => {
    fetchMock.mockResolvedValueOnce(answer('Tabby cats'));

    await generateChatTitle({ serverPort: 9000, messages: chat, prompt: 'Name this chat.' });

    const sent = JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string) as { messages: unknown[] };
    expect(sent.messages.at(-1)).toStrictEqual({ role: 'user', content: 'Name this chat.' });
  });

  it('cleans the model\'s text into the title', async () => {
    fetchMock.mockResolvedValueOnce(answer('Title: "Tabby cats."\nThat is my suggestion.'));

    await expect(generateChatTitle({ serverPort: 9000, messages: chat })).resolves.toBe('Tabby cats');
  });

  it('titles a chat the model said nothing for New Chat', async () => {
    fetchMock.mockResolvedValueOnce(answer(''));

    await expect(generateChatTitle({ serverPort: 9000, messages: chat })).resolves.toBe('New Chat');
  });

  it.each([
    [400, 'Bad Request', 'No running server found on port 9000. Start a server first.', undefined],
    [401, 'Unauthorized', 'this route needs the daemon\'s token', 'DAEMON_TOKEN_REQUIRED'],
    [500, 'Internal Server Error', 'llama-server returned 500 Internal Server Error: the template failed', undefined],
    [503, 'Service Unavailable', 'Failed to connect to llama-server on port 9000', undefined],
  ])('says a %i in its own sentence, by the status and not the daemon\'s reason', async (status, statusText, reason, type) => {
    fetchMock.mockResolvedValueOnce(refusal(status, statusText, reason, type));

    const failure = await generateChatTitle({ serverPort: 9000, messages: chat }).catch((e: unknown) => e);

    expect(failure).toBeInstanceOf(Error);
    expect(failure).not.toBeInstanceOf(TransportError);
    expect((failure as Error).message).toBe(`Title generation failed: ${statusText}`);
  });

  it('passes on a failure that is not a refusal as it came', async () => {
    const offline = new TypeError('Failed to fetch');
    fetchMock.mockRejectedValueOnce(offline);

    await expect(generateChatTitle({ serverPort: 9000, messages: chat })).rejects.toBe(offline);
  });

  it('fails when the model\'s text is thinking and nothing else', async () => {
    fetchMock.mockResolvedValueOnce(answer('<think>a title, hmm</think>'));

    await expect(generateChatTitle({ serverPort: 9000, messages: chat })).rejects.toThrow(
      'Model returned empty content for title generation.',
    );
  });
});
