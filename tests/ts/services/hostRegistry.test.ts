/**
 * The one file-name helper splits a path on the separators of the machine it
 * is a path on: the service's host (`contracts/host/machines.json`), or a
 * Hugging Face repository's `/`. Before the host is known a path is shown
 * whole, and the machine is "this computer".
 */

import { describe, it, expect } from 'vitest';

import { HF_REPO, fileName, thisMachine } from '../../../src/services/hostRegistry';
import type { Host } from '../../../src/types/generated/Host';
import { rust } from '../contracts/rustSource';

const hosts = JSON.parse(rust('contracts/host/machines.json')) as Host[];
const [mac, windows] = hosts;

describe('the one file-name helper', () => {
  it("names a file by its last name, split as the service's machine splits a path", () => {
    expect(fileName('/home/m/src/gglib/', mac)).toBe('gglib');
    expect(fileName('/models/mmproj-F16.gguf', mac)).toBe('mmproj-F16.gguf');
    expect(fileName('C:\\Users\\m\\notes', windows)).toBe('notes');
    expect(fileName('C:\\Users\\m\\My Models\\a b.gguf', windows)).toBe('a b.gguf');
    expect(fileName('C:/Users/m/q.gguf', windows)).toBe('q.gguf');
    // On a Mac a backslash is part of a name.
    expect(fileName('/m/a\\b.gguf', mac)).toBe('a\\b.gguf');
    expect(fileName('/', mac)).toBe('/');
    expect(fileName('model.gguf', mac)).toBe('model.gguf');
  });

  it("splits a repository's path on / on every machine", () => {
    expect(fileName('sub/dir/model-Q4_K_M.gguf', HF_REPO)).toBe('model-Q4_K_M.gguf');
  });

  it('shows a path whole, and names the machine as any computer, before the host is known', () => {
    expect(fileName('C:\\Users\\m\\q.gguf', null)).toBe('C:\\Users\\m\\q.gguf');
    // The word Linux's machine has, which any OS gglib does not name reads as.
    const linux = hosts.find((h) => h.os === 'linux') ?? null;
    expect(linux).not.toBeNull();
    expect(thisMachine(null)).toEqual(thisMachine(linux));
    expect(thisMachine(null)).toEqual({ title: 'This computer', phrase: 'this computer' });
  });
});
