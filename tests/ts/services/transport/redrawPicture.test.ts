/**
 * A picture drawn again: what this machine is sent, how the stream it
 * answers is read, and the newer picture kept on its tool's row.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { createApiTransport } from '../../../../src/services/transport/api';
import { keepPicture } from '../../../../src/services/transport/api/chat';
import { setApiSession } from '../../../../src/services/transport/api/client';
import {
  redrawPicture,
  type PictureRedraw,
  type RedrawProgress,
} from '../../../../src/services/transport/api/images';

let fetchSpy: ReturnType<typeof vi.fn>;

function sse(frames: Array<[string, unknown]>): Response {
  const text = frames.map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('');
  return new Response(text, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

function answer(response: Response | (() => Promise<Response>)) {
  fetchSpy = vi.fn(typeof response === 'function' ? response : async () => response);
  vi.stubGlobal('fetch', fetchSpy);
}

/** Everything a redraw reported, in order, once it is over. */
function followed(redraw: PictureRedraw): Promise<{ said: string[]; stop: () => void }> {
  return new Promise((resolve) => {
    const said: string[] = [];
    const over = (line: string) => {
      said.push(line);
      resolve({ said, stop });
    };
    const stop = redrawPicture(redraw, {
      onProgress: (progress: RedrawProgress) => said.push(`progress ${JSON.stringify(progress)}`),
      onDrawn: (png) => over(`drawn ${png}`),
      onFailed: (message) => over(`failed ${message}`),
    });
  });
}

const FIX: PictureRedraw = { prompt: 'a hand', image: 'UElD', mask: 'TUFTSw==', strength: 0.75, model: 5 };

beforeEach(() => setApiSession('', 'page-key'));
afterEach(() => {
  vi.unstubAllGlobals();
  setApiSession('', undefined);
});

describe('redrawPicture', () => {
  it('posts the picture, mask, strength and model as a stream, with the page\'s credential', async () => {
    answer(sse([['image_generation.completed', { b64_json: 'RE9ORQ==' }]]));

    await followed(FIX);

    const [url, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/images/generations');
    expect(init.method).toBe('POST');
    expect(init.headers).toMatchObject({ Authorization: 'Bearer page-key', Accept: 'text/event-stream' });
    expect(JSON.parse(init.body as string)).toEqual({
      prompt: 'a hand',
      image: 'UElD',
      mask: 'TUFTSw==',
      strength: 0.75,
      model: '5',
      stream: true,
    });
  });

  it('sends no mask and no model for a whole redraw with the machine\'s own choice', async () => {
    answer(sse([['image_generation.completed', { b64_json: 'RE9ORQ==' }]]));

    await followed({ prompt: 'a fox', image: 'UElD', strength: 0.4 });

    const body = JSON.parse((fetchSpy.mock.calls[0] as [string, RequestInit])[1].body as string);
    expect(body).toEqual({ prompt: 'a fox', image: 'UElD', strength: 0.4, stream: true });
  });

  it('reports each stage and step with its frame, then the picture, and nothing after it', async () => {
    answer(
      sse([
        ['image_generation.progress', { type: 'image_generation.progress', stage: 'queued', position: 2 }],
        ['image_generation.progress', { stage: 'loading' }],
        ['image_generation.progress', { stage: 'sampling', pass: 1, step: 3, total: 20, frame_b64: 'RlJBTUU=' }],
        ['image_generation.partial_image', { b64_json: 'UEFSVA==' }],
        ['image_generation.progress', { stage: 'decoding' }],
        ['image_generation.completed', { b64_json: 'RE9ORQ==', size: '64x64' }],
        ['image_generation.progress', { stage: 'finishing' }],
      ]),
    );

    const { said } = await followed(FIX);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(said).toEqual([
      'progress {"stage":"queued","position":2}',
      'progress {"stage":"loading"}',
      'progress {"stage":"sampling","step":3,"total":20,"frame":"RlJBTUU="}',
      'progress {"stage":"decoding"}',
      'drawn RE9ORQ==',
    ]);
  });

  it('fails in the machine\'s words for an error event, once', async () => {
    answer(
      sse([
        ['image_generation.progress', { stage: 'loading' }],
        ['error', { error: { message: 'the mask has no white in it', code: 'invalid_request' } }],
      ]),
    );

    const { said } = await followed(FIX);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(said).toEqual(['progress {"stage":"loading"}', 'failed the mask has no white in it']);
  });

  it('fails when the stream ends with no picture, and when the request is refused', async () => {
    answer(sse([['image_generation.progress', { stage: 'loading' }]]));
    expect((await followed(FIX)).said.at(-1)).toBe('failed The render ended without a picture.');

    answer(
      new Response(JSON.stringify({ error: { message: 'no image model', code: 'drawing_unavailable' } }), {
        status: 400,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const refused = (await followed(FIX)).said;
    expect(refused).toHaveLength(1);
    expect(refused[0]).toMatch(/^failed /);
  });

  it('stops by leaving the stream, and says nothing more', async () => {
    let left = false;
    answer(
      (...args: unknown[]) =>
        new Promise<Response>((_, reject) => {
          const { signal } = args[1] as RequestInit;
          signal?.addEventListener('abort', () => {
            left = true;
            reject(Object.assign(new Error('aborted'), { name: 'AbortError' }));
          });
        }),
    );
    const said: string[] = [];
    const stop = redrawPicture(FIX, {
      onProgress: () => said.push('progress'),
      onDrawn: () => said.push('drawn'),
      onFailed: () => said.push('failed'),
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    stop();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(left).toBe(true);
    expect(said).toEqual([]);
  });
});

describe('keepPicture', () => {
  it('posts the two ids to the chat and answers the row\'s images', async () => {
    const row = [
      { id: 'a'.repeat(64), mime: 'image/png', width: 64, height: 64 },
      { id: 'b'.repeat(64), mime: 'image/png', width: 64, height: 64 },
    ];
    answer(new Response(JSON.stringify(row), { status: 200, headers: { 'content-type': 'application/json' } }));

    const kept = await keepPicture(7, { from: row[0].id, image: row[1].id });

    expect(kept).toEqual(row);
    const [url, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/conversations/7/pictures');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toEqual({ from: row[0].id, image: row[1].id });
  });

  it('is on the transport object with redrawPicture', () => {
    const transport = createApiTransport();
    expect(transport.keepPicture).toBe(keepPicture);
    expect(transport.redrawPicture).toBe(redrawPicture);
  });
});
