/**
 * The composer's Draw button: greyed, with the reason, until the machine
 * that would draw answers that it can, and whenever it answers that it
 * cannot; armed by a press for the open chat alone; saying `draw` on a send
 * only while armed; and no longer armed once the turn that said it is
 * accepted, while a refused one leaves it armed to say it again.
 *
 * The transport is the real one over a fake `fetch`, so what is asked is
 * what a daemon would be: this machine's route, told what the page knows of
 * the chat's model, or the far machine's for a far chat. A send is `forSend`
 * asked, and its turn is accepted when the `accepted` of that answer is
 * called, as the runtime does once the daemon has taken the turn.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import {
  DRAW_CHECKING,
  DRAW_UNAVAILABLE,
  DRAW_UNCHECKED,
  useDrawSwitch,
  type DrawSwitchChat,
} from '../../../src/hooks/useDrawSwitch';
import type { DrawingAvailability } from '../../../src/types/generated/DrawingAvailability';
import type { ModelRef } from '../../../src/types/generated/ModelRef';

const CAN: DrawingAvailability = { available: true, model: 'flux-schnell' };
const NO_MODEL: DrawingAvailability = {
  available: false,
  code: 'drawing_unavailable',
  reason: 'no image model is installed; download one first',
};
const paired: ModelRef = { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 };

/** What each route answers; a test changes it as it goes. */
let answers: { here: DrawingAvailability | Response; far: DrawingAvailability | Response };
/** Every question asked, as its URL. */
let asked: string[];
/** Holds every answer back until it settles, when set. */
let gate: Promise<void> | null;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

beforeEach(() => {
  answers = { here: CAN, far: CAN };
  asked = [];
  gate = null;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      asked.push(url);
      await gate;
      const answer = url.startsWith('/api/remote/images/drawing') ? answers.far : answers.here;
      return answer instanceof Response ? answer.clone() : json(answer);
    }),
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
});

const here = (conversationId: number | null, extra: Partial<DrawSwitchChat> = {}): DrawSwitchChat => ({
  conversationId,
  far: false,
  ...extra,
});

/** Mount the button on `chat` and wait for its machine's answer. */
async function mounted(chat: DrawSwitchChat) {
  const hook = renderHook((props: DrawSwitchChat) => useDrawSwitch(props), { initialProps: chat });
  await waitFor(() => expect(hook.result.current.reason).not.toBe(DRAW_CHECKING));
  return hook;
}

describe('useDrawSwitch, whether a message can draw', () => {
  it('is greyed and says it is checking until the answer comes, then offers the model that would draw', async () => {
    let release = () => {};
    gate = new Promise((resolve) => (release = resolve));
    const hook = renderHook(() => useDrawSwitch(here(1)));

    expect(hook.result.current).toMatchObject({ available: false, armed: false, reason: DRAW_CHECKING });
    act(() => hook.result.current.toggle());
    expect(hook.result.current.armed).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    release();
    await waitFor(() => expect(hook.result.current.available).toBe(true));
    expect(hook.result.current.reason).toBeUndefined();
    expect(hook.result.current.model).toBe('flux-schnell');
    // The press made while it was greyed armed nothing.
    expect(hook.result.current.armed).toBe(false);
  });

  it('stays greyed with the machine\'s reason when it cannot, and no press arms it', async () => {
    answers.here = NO_MODEL;
    const hook = await mounted(here(1));

    expect(hook.result.current).toMatchObject({ available: false, armed: false, reason: NO_MODEL.reason });
    expect(hook.result.current.model).toBeUndefined();
    act(() => hook.result.current.toggle());
    expect(hook.result.current.armed).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('says so when a machine answers no and gives no reason', async () => {
    answers.here = { available: false };
    const hook = await mounted(here(1));
    expect(hook.result.current).toMatchObject({ available: false, reason: DRAW_UNAVAILABLE });
  });

  it('is greyed, saying why, when the question could not be asked', async () => {
    answers.far = json({ error: 'no route GET /v1/images/drawing', status: 404, type: 'not_found' }, 404);
    const hook = await mounted({ conversationId: 1, far: true });

    expect(hook.result.current.available).toBe(false);
    expect(hook.result.current.reason).toContain(DRAW_UNCHECKED);
    expect(hook.result.current.reason).toContain('no route GET /v1/images/drawing');
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('asks this machine, telling it what is known of the chat\'s model and nothing that is not', async () => {
    await mounted(here(1));
    expect(asked).toEqual(['/api/images/drawing']);

    asked = [];
    await mounted(here(1, { supportsToolCalls: false }));
    expect(asked).toEqual(['/api/images/drawing?calls_tools=false']);

    asked = [];
    await mounted(here(1, { supportsToolCalls: true, paired }));
    expect(asked).toEqual(['/api/images/drawing?far=true&calls_tools=true']);
  });

  it('asks the far machine for a far chat, and tells it nothing of this machine\'s model', async () => {
    answers.here = NO_MODEL;
    const hook = await mounted({ conversationId: 1, far: true, supportsToolCalls: false, paired });

    expect(asked).toEqual(['/api/remote/images/drawing']);
    expect(hook.result.current.available).toBe(true);
  });

  it('asks again as another chat is opened, when the model\'s tools are learned, and when told to', async () => {
    const hook = await mounted(here(1));
    expect(asked).toHaveLength(1);

    hook.rerender(here(2));
    await waitFor(() => expect(asked).toHaveLength(2));
    hook.rerender(here(2, { supportsToolCalls: true }));
    await waitFor(() => expect(asked).toHaveLength(3));
    hook.rerender(here(2, { supportsToolCalls: true, refresh: 7 }));
    await waitFor(() => expect(asked).toHaveLength(4));
    // The same chat rendered again asks nothing.
    hook.rerender(here(2, { supportsToolCalls: true, refresh: 7 }));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(asked).toHaveLength(4);
  });

  it('shows no answer to another question: a chat moved to a model that calls no tools is greyed until its own comes', async () => {
    const hook = await mounted(here(1));
    expect(hook.result.current.available).toBe(true);
    act(() => hook.result.current.toggle());
    expect(hook.result.current.armed).toBe(true);

    let release = () => {};
    gate = new Promise((resolve) => (release = resolve));
    answers.here = { available: false, reason: 'this chat\'s model calls no tools, so it cannot draw' };
    hook.rerender(here(1, { supportsToolCalls: false }));

    // The earlier yes was about another model: not shown, and nothing is said.
    expect(hook.result.current).toMatchObject({ available: false, armed: false, reason: DRAW_CHECKING });
    expect(hook.result.current.forSend()).toBeUndefined();
    release();
    await waitFor(() => expect(hook.result.current.reason).toBe('this chat\'s model calls no tools, so it cannot draw'));
    expect(hook.result.current.armed).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
  });
});

describe('useDrawSwitch, armed for one message', () => {
  it('a press arms it, and a second press disarms it', async () => {
    const hook = await mounted(here(1));
    expect(hook.result.current.armed).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    act(() => hook.result.current.toggle());
    expect(hook.result.current.armed).toBe(true);
    await waitFor(() => expect(hook.result.current.forSend()).toBeDefined());

    act(() => hook.result.current.toggle());
    expect(hook.result.current.armed).toBe(false);
    await waitFor(() => expect(hook.result.current.forSend()).toBeUndefined());
  });

  it('is no longer armed once the turn that said draw is accepted, and the next send says nothing', async () => {
    const hook = await mounted(here(1));
    act(() => hook.result.current.toggle());
    await waitFor(() => expect(hook.result.current.forSend()).toBeDefined());

    const said = hook.result.current.forSend()!;
    act(() => said.accepted());

    expect(hook.result.current.armed).toBe(false);
    await waitFor(() => expect(hook.result.current.forSend()).toBeUndefined());
  });

  it('stays armed when its turn is refused, so the next send says draw again', async () => {
    const hook = await mounted(here(1));
    act(() => hook.result.current.toggle());
    await waitFor(() => expect(hook.result.current.forSend()).toBeDefined());

    // A refused send asks, and never calls `accepted`.
    hook.result.current.forSend();
    hook.rerender(here(1));

    expect(hook.result.current.armed).toBe(true);
    expect(hook.result.current.forSend()).toBeDefined();
  });

  it('arms the open chat alone: another chat, and the same id on the other machine, are not armed', async () => {
    const hook = await mounted(here(1));
    act(() => hook.result.current.toggle());
    expect(hook.result.current.armed).toBe(true);

    hook.rerender(here(2));
    await waitFor(() => expect(hook.result.current.forSend()).toBeUndefined());
    expect(hook.result.current.armed).toBe(false);

    hook.rerender({ conversationId: 1, far: true });
    await waitFor(() => expect(hook.result.current.available).toBe(true));
    expect(hook.result.current.armed).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    hook.rerender(here(1));
    await waitFor(() => expect(hook.result.current.armed).toBe(true));
  });

  it('a chat not yet made is armed as the new one, and its accepted turn disarms it though the chat has an id by then', async () => {
    const hook = await mounted(here(null));
    act(() => hook.result.current.toggle());
    await waitFor(() => expect(hook.result.current.forSend()).toBeDefined());
    const said = hook.result.current.forSend()!;

    // The send made the conversation before its run was accepted.
    hook.rerender(here(100));
    act(() => said.accepted());
    hook.rerender(here(null));

    expect(hook.result.current.armed).toBe(false);
    await waitFor(() => expect(hook.result.current.forSend()).toBeUndefined());
  });
});
