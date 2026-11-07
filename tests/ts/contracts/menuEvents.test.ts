/**
 * The native menus' event names and place ids, against the Rust that sends
 * them.
 *
 * `MENU_EVENTS` is what the menu bridge listens for and `MENU_PLACES` the
 * places its `menu:navigate` handler knows; `src-tauri/src/app/events.rs` is
 * what the menu and the tray emit, its `names` block the events and its
 * `places` block the ids `menu:navigate` carries. A name or a place on one
 * side only is a menu item that does nothing, silently. Anchored on each
 * module, not on a bare search of the file, so a literal elsewhere cannot
 * stand in for one here; `#[cfg(target_os = "macos")]` guards are part of
 * the module and are read through.
 */

import { describe, it, expect } from 'vitest';
import { MENU_EVENTS, MENU_PLACES } from '../../../src/services/platform/menuEvents';
import { rust, withoutComments } from './rustSource';

/** The body of `pub(crate) mod <name> {` in events.rs. */
function rustModule(name: string): string {
  const source = withoutComments(rust('src-tauri/src/app/events.rs'));
  const open = source.indexOf(`pub(crate) mod ${name} {`);
  if (open < 0) throw new Error(`events.rs: no \`pub(crate) mod ${name} {\` block`);
  const close = source.indexOf('\n}', open);
  if (close < 0) throw new Error(`events.rs: \`${name}\` block never closes`);
  return source.slice(open, close);
}

/** Every string literal a `pub(crate) const` in the module declares. */
function literals(name: string): string[] {
  const found = [...rustModule(name).matchAll(/pub\(crate\) const [A-Z_]+: &str = "([^"]+)";/g)].map((m) => m[1]);
  if (found.length === 0) throw new Error(`events.rs: the \`${name}\` block declares nothing`);
  return found.sort();
}

describe('the native menu events', () => {
  it('the bridge listens for exactly the names Rust sends', () => {
    expect([...Object.values(MENU_EVENTS)].sort()).toEqual(literals('names'));
  });

  it('the bridge knows exactly the places Rust names', () => {
    expect([...MENU_PLACES].sort()).toEqual(literals('places'));
  });

  it('every Rust name and place is declared once', () => {
    for (const block of ['names', 'places']) {
      const declared = literals(block);
      expect(new Set(declared).size).toBe(declared.length);
    }
  });

  it('navigation is one event, and the endpoint is turned on and off in Rust', () => {
    expect(literals('names')).toEqual([
      'menu:copy-to-clipboard',
      'menu:hide-inspector',
      'menu:load-model',
      'menu:load-with-options',
      'menu:navigate',
      'menu:remove-model',
      'menu:unload-model',
    ]);
  });
});
