/**
 * Where an engine's install stands against its pinned release, in the words
 * the Settings engine card uses, for llama.cpp and stable-diffusion.cpp alike.
 *
 * The standing is the daemon's (`PinState`), the same one
 * `gglib config llama status` and `gglib config sd status` print in their
 * own words, and so is whether the update is a download.
 */

import type { PinState } from '../../types/setup';

/** What a card says about the pin, and whether it offers the update. */
export interface PinStanding {
  /** One sentence; null when nothing is installed, where the card says so. */
  text: string | null;
  /**
   * The release an Update button installs, when one is offered: what is
   * installed is older, and the update is a download, the daemon's verdict.
   */
  update: string | null;
  /**
   * Said instead of the button for an update the command line builds: over
   * a source build, or on a machine with no download.
   */
  fromTerminal: string | null;
}

/** Where `pin` stands, for the engine whose update command is `command`. */
export function pinStanding(pin: PinState, command: string): PinStanding {
  const target = pin.targetRelease ?? "upstream's newest";
  const installed = pin.installedRelease ?? 'unknown';
  switch (pin.standing) {
    case 'missing':
      return { text: null, update: null, fromTerminal: null };
    case 'atPin':
      return { text: `Pinned to ${target}, which is the one installed.`, update: null, fromTerminal: null };
    case 'newer':
      return {
        text: `Pinned to ${target}. The one installed, ${installed}, is newer, and is kept.`,
        update: null,
        fromTerminal: null,
      };
    case 'unknown':
      return {
        text:
          pin.targetRelease === null
            ? "Following upstream's newest release, which is not compared here."
            : `Pinned to ${target}. What is installed cannot be compared with it.`,
        update: null,
        fromTerminal: null,
      };
    case 'older':
      return {
        text: `Pinned to ${target}. The one installed, ${installed}, is older.`,
        update: pin.updateDownloads ? target : null,
        fromTerminal: pin.updateDownloads
          ? null
          : `The update is a source build, run from a terminal: ${command}.`,
      };
  }
}
