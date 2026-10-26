/**
 * Where an engine stands against its pinned release, as both Settings cards
 * say it, and when each offers the update.
 */

import { describe, it, expect } from 'vitest';

import { pinStanding } from '../../../src/components/SettingsModal/pinStanding';
import type { PinState } from '../../../src/types/setup';

const COMMAND = 'gglib config llama update';

function pin(overrides: Partial<PinState>): PinState {
  return {
    standing: 'atPin',
    installedRelease: 'b11429',
    targetRelease: 'b11429',
    built: false,
    rebuilds: false,
    updateDownloads: false,
    ...overrides,
  };
}

describe('pinStanding', () => {
  it('says nothing of an engine that is not installed', () => {
    expect(pinStanding(pin({ standing: 'missing', installedRelease: null }), COMMAND)).toEqual({
      text: null,
      update: null,
      fromTerminal: null,
    });
  });

  it('offers nothing at the pin, and keeps a newer release', () => {
    expect(pinStanding(pin({}), COMMAND)).toEqual({
      text: 'Pinned to b11429, which is the one installed.',
      update: null,
      fromTerminal: null,
    });
    expect(pinStanding(pin({ standing: 'newer', installedRelease: 'b11870' }), COMMAND)).toEqual({
      text: 'Pinned to b11429. The one installed, b11870, is newer, and is kept.',
      update: null,
      fromTerminal: null,
    });
  });

  it('offers the download over an older install when the daemon says the update downloads', () => {
    expect(
      pinStanding(pin({ standing: 'older', installedRelease: 'b10327', updateDownloads: true }), COMMAND),
    ).toEqual({
      text: 'Pinned to b11429. The one installed, b10327, is older.',
      update: 'b11429',
      fromTerminal: null,
    });
  });

  it('sends an update the command line builds to the terminal, offering no download', () => {
    expect(
      pinStanding(
        pin({ standing: 'older', installedRelease: 'b10327', built: true, rebuilds: true, updateDownloads: false }),
        COMMAND,
      ),
    ).toEqual({
      text: 'Pinned to b11429. The one installed, b10327, is older.',
      update: null,
      fromTerminal: 'The update is a source build, run from a terminal: gglib config llama update.',
    });
  });

  it('says what it cannot compare, and why', () => {
    expect(pinStanding(pin({ standing: 'unknown', targetRelease: null }), COMMAND).text).toBe(
      "Following upstream's newest release, which is not compared here.",
    );
    expect(pinStanding(pin({ standing: 'unknown', installedRelease: null }), COMMAND).text).toBe(
      'Pinned to b11429. What is installed cannot be compared with it.',
    );
  });
});
