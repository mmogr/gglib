/**
 * The two helpers for a caught value: is it an abort, and what does it say.
 */

import { describe, it, expect } from 'vitest';
import { formatError, isAbortError } from '../../../src/utils/errors';

describe('isAbortError', () => {
  it('recognises what an aborted signal rejects with', () => {
    const controller = new AbortController();
    controller.abort();

    expect(isAbortError(controller.signal.reason)).toBe(true);
  });

  it('recognises what a fetch rejects with when its signal fires', async () => {
    const controller = new AbortController();
    const request = new Promise((_resolve, reject) => {
      controller.signal.addEventListener('abort', () => reject(controller.signal.reason));
    });
    controller.abort();

    expect(isAbortError(await request.catch((e: unknown) => e))).toBe(true);
  });

  it('recognises an Error named AbortError', () => {
    expect(isAbortError(Object.assign(new Error('aborted'), { name: 'AbortError' }))).toBe(true);
  });

  it('does not take another error for an abort', () => {
    expect(isAbortError(new Error('AbortError'))).toBe(false);
    expect(isAbortError(new TypeError('network error'))).toBe(false);
    expect(isAbortError(Object.assign(new Error('too slow'), { name: 'TimeoutError' }))).toBe(false);
  });

  it('answers false, without throwing, for a rejection that is not an Error at all', () => {
    for (const thrown of [undefined, null, 'AbortError', 0, { name: 'AbortError' }]) {
      expect(isAbortError(thrown)).toBe(false);
    }
  });
});

describe('formatError', () => {
  it('is an Error\'s message', () => {
    expect(formatError(new Error('the daemon said no'))).toBe('the daemon said no');
    expect(formatError(new TypeError('network error'))).toBe('network error');
  });

  it('is a thrown string, as it is', () => {
    expect(formatError('port 9000 is in use')).toBe('port 9000 is in use');
    expect(formatError('')).toBe('');
  });

  it('is a thrown object\'s JSON, never "[object Object]"', () => {
    expect(formatError({ code: 'busy', detail: 'try later' })).toBe('{"code":"busy","detail":"try later"}');
  });

  it('is what String gives for a number, a boolean and null', () => {
    for (const thrown of [42, false, null]) {
      expect(formatError(thrown)).toBe(String(thrown));
    }
  });

  it('is a string for a value JSON has no form for', () => {
    const named = Symbol('why');
    const fn = () => 1;

    expect(formatError(undefined)).toBe('undefined');
    expect(formatError(named)).toBe('Symbol(why)');
    expect(formatError(fn)).toBe(String(fn));
  });

  it('is what String gives for a value JSON refuses', () => {
    const loop: Record<string, unknown> = {};
    loop.self = loop;

    expect(formatError(loop)).toBe('[object Object]');
    expect(formatError(10n)).toBe('10');
  });
});
