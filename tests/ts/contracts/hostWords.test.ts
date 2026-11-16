/**
 * The app names the machine the service runs on in the service's words.
 * `gglib_core::host` decides what the machine is called ("Mac", "PC",
 * "computer") and `GET /api/version` serves it as `host`; `gglib-core`'s
 * host tests record every OS with its `Host` in
 * `contracts/host/machines.json`. The one helper (`thisMachine` in
 * `src/services/hostRegistry.ts`) turns a host into "This Mac", "This PC" or
 * "This computer", and no place in `src` names the machine itself.
 *
 * A browser tab on another machine reads the service's answer too, so the
 * words are the service's machine's, never the browser's.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

import { describe, it, expect } from 'vitest';

import type { Host } from '../../../src/types/generated/Host';

import { rust } from './rustSource';

const REPO_ROOT = resolve(import.meta.dirname, '../../..');

/**
 * The one helper's module, imported when its test runs rather than with the
 * file, so a missing helper fails that test alone and the others still say
 * what they find.
 */
const HELPER = '../../../src/services/hostRegistry';

/** The recorded hosts, or none when the file is not there. */
function recorded(): Host[] {
  try {
    return JSON.parse(rust('contracts/host/machines.json')) as Host[];
  } catch {
    return [];
  }
}

/** What each OS's machine is called at the start of a line. */
const TITLES: Record<string, string> = {
  macos: 'This Mac',
  windows: 'This PC',
  linux: 'This computer',
};

/** Every `.ts` and `.tsx` file under `dir`, the generated bindings left out. */
function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === 'generated' ? [] : sources(path);
    return /\.tsx?$/.test(name) ? [path] : [];
  });
}

/** `source` with its block comments and whole-line comments removed. */
function code(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .split('\n')
    .map((line) => (line.trim().startsWith('//') ? '' : line))
    .join('\n');
}

describe("the machine is named in the service's words", () => {
  it("the service's build answer carries its host", () => {
    expect(rust('src/types/generated/VersionDto.ts')).toMatch(/\bhost: Host\b/);
  });

  it('records every OS with what its machine is called', () => {
    expect(recorded().map((h) => [h.os, h.machine])).toEqual([
      ['macos', 'Mac'],
      ['windows', 'PC'],
      ['linux', 'computer'],
    ]);
  });

  it('the one helper says "This Mac", "This PC" or "This computer" from the host alone', async () => {
    const { thisMachine } = (await import(/* @vite-ignore */ HELPER)) as typeof import('../../../src/services/hostRegistry');
    const hosts = recorded();
    expect(hosts.length).toBeGreaterThan(0);
    for (const host of hosts) {
      expect(thisMachine(host)).toEqual({ title: TITLES[host.os], phrase: TITLES[host.os].replace('This', 'this') });
    }
  });

  it('no place in src names the machine in words of its own', () => {
    const named = sources(resolve(REPO_ROOT, 'src')).flatMap((path) =>
      code(readFileSync(path, 'utf8'))
        .split('\n')
        // The window's own OS (the traffic lights, the menu bar's keys) is
        // the browser's to say, and is not a name for the service's machine.
        .filter((line) => !line.includes('navigator.platform'))
        .filter((line) => /\bMac\b|\b[Tt]his (PC|computer)\b/.test(line))
        .map((line) => `${relative(REPO_ROOT, path)}: ${line.trim()}`),
    );
    expect(named).toEqual([]);
  });
});
