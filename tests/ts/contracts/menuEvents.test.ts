/**
 * The native menu's event names, against the Rust that sends them.
 *
 * `MENU_EVENTS` is what the menu bridge listens for; `src-tauri/src/app/events.rs`
 * is what the menu, the tray and the proxy actions emit. A name present on
 * one side only is a menu item that does nothing, silently. Anchored on the
 * `names` module, not on a bare search of the file, so a literal elsewhere
 * cannot stand in for one here; `#[cfg(target_os = "macos")]` guards are
 * part of the module and are read through.
 */

import { describe, it, expect } from 'vitest';
import { MENU_EVENTS } from '../../../src/services/platform/menuEvents';
import { rust } from './rustSource';

/** The `names` module's body, the one block that declares the event names. */
function namesModule(): string {
  const source = rust('src-tauri/src/app/events.rs');
  const open = source.indexOf('pub(crate) mod names {');
  if (open < 0) throw new Error('events.rs: no `pub(crate) mod names {` block');
  const close = source.indexOf('\n}', open);
  if (close < 0) throw new Error('events.rs: `names` block never closes');
  return source.slice(open, close);
}

/** Every `"menu:…"` literal the module declares. */
function rustMenuEvents(): string[] {
  const literals = [...namesModule().matchAll(/"(menu:[a-z-]+)"/g)].map((m) => m[1]);
  if (literals.length === 0) throw new Error('events.rs: the `names` block declares no menu event');
  return literals.sort();
}

describe('the native menu events', () => {
  it('the bridge listens for exactly the names Rust sends', () => {
    expect([...Object.values(MENU_EVENTS)].sort()).toEqual(rustMenuEvents());
  });

  it('every Rust name is declared once', () => {
    const names = rustMenuEvents();
    expect(new Set(names).size).toBe(names.length);
  });
});
