/**
 * The second-launch event, against the Rust that sends it.
 *
 * `src-tauri/src/second_launch.rs` emits `EVENT` to the main window with a
 * `SecondLaunchPayload`; `src/services/platform/secondLaunch.ts` listens for
 * `SECOND_LAUNCH_EVENT` and reads a `SecondLaunchPayload`. A name on one side
 * only is a launch nobody hears, and a field on one side only is one read as
 * `undefined`. Rust's own test pins the payload's JSON to `args` and `cwd`.
 */

import { describe, it, expect } from 'vitest';
import { SECOND_LAUNCH_EVENT } from '../../../src/services/platform/secondLaunch';
import { rust, structBody, withoutComments } from './rustSource';

const SOURCE = 'src-tauri/src/second_launch.rs';

describe('the second-launch event', () => {
  it('is listened for by the name Rust sends', () => {
    const found = [...withoutComments(rust(SOURCE)).matchAll(/^pub\(crate\) const EVENT: &str = "([^"]+)";$/gm)];
    expect(found.map((m) => m[1])).toEqual([SECOND_LAUNCH_EVENT]);
  });

  it('carries the fields the page reads, the arguments and the folder', () => {
    const fields = [...structBody(withoutComments(rust(SOURCE)), 'SecondLaunchPayload').matchAll(/^\s*pub\(crate\) (\w+): ([^,]+),$/gm)]
      .map((m) => `${m[1]}: ${m[2]}`);
    expect(fields).toEqual(['args: Vec<String>', 'cwd: String']);
  });
});
