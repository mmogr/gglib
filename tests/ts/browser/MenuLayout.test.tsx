/**
 * A menu's rows as an engine lays them out: each row under the one before
 * it, its label inside it, however long the label is.
 *
 * jsdom has no layout, so none of this can be read there. A menu hangs from
 * its button as an absolutely positioned box, which is as narrow as its
 * words allow unless it is given a width, and a row is a button, which is
 * one line tall and centres its text unless it is told otherwise: together
 * they wrap a label of two words onto two centred lines and draw the second
 * over the next row (https://github.com/mmogr/gglib/issues/2203).
 */

import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { cleanup, fireEvent, render } from '@testing-library/react';
import '../../../src/styles/tailwind.css';
import { Menu, type MenuItem } from '../../../src/components/ui/Menu';

/** The daemon's reason a message cannot draw yet, as the submenu's note. */
const NOTE =
  'This machine has 3 image models that can draw (flux-2-klein-9b, sdxl-turbo-1.0-fp16, Z-Image-Turbo); name the one to draw with, or set a default with `gglib config set default-image-model-id <name>`';

/** Image models, as `composerMenus` labels them: a name and its family. */
const MODELS = ['Z-Image-Turbo · Z-Image', 'sdxl-turbo-1.0-fp16 · Stable Diffusion XL', 'flux-2-klein-9b · FLUX.2 klein'];

/** A label no menu is wide enough for on one line. */
const LONG = 'A model whose name goes on for longer than any menu is wide · a family named at length · loaded · incomplete';

/** A row's label, as CSS names its font. */
const ROW_FONT = "14px 'IBM Plex Sans Variable'";

/** The widest a menu is, in px: 24rem. */
const WIDEST = 384;
/** The widest a submenu is, in px: 20rem. */
const WIDEST_SUB = 320;
/**
 * The room the composer's `[+]` has to its right at the app's smallest
 * window, in px: 800 (`minWidth` in src-tauri/tauri.conf.json), less the
 * sidebar's 240 and the 32 the composer sets `[+]` in from the chat's edge.
 * The chat clips what is drawn past it.
 */
const ROOM = 800 - 240 - 32;

const choice = (label: string, checked = false): MenuItem => ({ label, checked, onPick: () => {} });

/** The composer's `[+]`, with Draw a picture's submenu holding `rows`. */
const plusMenu = (rows: MenuItem[]) => (
  <div style={{ position: 'fixed', left: 40, bottom: 40 }}>
    <Menu
      label="+"
      name="Add to the message"
      iconOnly
      align="start"
      side="above"
      items={[
        { label: 'Add image…', onPick: () => {} },
        { label: 'Draw a picture', submenu: rows },
        { label: 'Tools', submenu: [{ label: 'No tools registered', note: true }] },
      ]}
    />
  </div>
);

/** Open `[+]` › Draw a picture, and give back the submenu. */
const openDraw = (rows: MenuItem[]): HTMLElement => {
  const view = render(plusMenu(rows));
  fireEvent.click(view.getByRole('button', { name: 'Add to the message' }));
  fireEvent.click(view.getByRole('menuitem', { name: 'Draw a picture' }));
  return view.getByRole('menu', { name: 'Draw a picture' });
};

const rowsOf = (menu: HTMLElement) => Array.from(menu.querySelectorAll<HTMLElement>(':scope > [role="menuitemradio"]'));

/** Where a line of a label is drawn. */
interface Line {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

/** The lines a row's label is drawn on, read from its text alone (a tick is no part of a line). */
const linesOf = (row: HTMLElement): Line[] => {
  const lines: Line[] = [];
  const texts = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
  for (let text = texts.nextNode(); text; text = texts.nextNode()) {
    const range = document.createRange();
    range.selectNodeContents(text);
    for (const rect of range.getClientRects()) {
      if (rect.width === 0) continue;
      const line = lines.find((known) => Math.abs(known.top - rect.top) < 1);
      if (line) {
        line.left = Math.min(line.left, rect.left);
        line.right = Math.max(line.right, rect.right);
      } else {
        lines.push({ top: rect.top, bottom: rect.bottom, left: rect.left, right: rect.right });
      }
    }
  }
  return lines;
};

/** Every row sits under the one before it, and holds every line of its own label. */
const expectRowsApart = (menu: HTMLElement) => {
  const rows = rowsOf(menu);
  expect(rows.length).toBeGreaterThan(1);
  rows.forEach((row, index) => {
    const box = row.getBoundingClientRect();
    const next = rows[index + 1]?.getBoundingClientRect();
    if (next) expect(box.bottom, `"${row.textContent}" ends above the next row`).toBeLessThanOrEqual(next.top + 0.5);
    for (const line of linesOf(row)) {
      expect(line.top, `"${row.textContent}" starts inside its row`).toBeGreaterThanOrEqual(box.top - 0.5);
      expect(line.bottom, `"${row.textContent}" ends inside its row`).toBeLessThanOrEqual(box.bottom + 0.5);
    }
  });
  const last = rows[rows.length - 1].getBoundingClientRect();
  expect(last.bottom, 'the last row ends inside the menu').toBeLessThanOrEqual(menu.getBoundingClientRect().bottom + 0.5);
};

describe('Menu rows in a browser engine', () => {
  beforeAll(async () => {
    // A window as wide as the app's, so the menu's own width is what is read.
    await page.viewport(1280, 800);
    // The app's own font, loaded before a width is read: a fallback's letters
    // are another width, and `fonts.ready` waits for nothing until some text
    // has asked for a font.
    await document.fonts.load(ROW_FONT, MODELS.join(' '));
    expect([...document.fonts].filter((face) => face.status === 'loaded').map((face) => face.family.replace(/["']/g, ''))).toContain(
      'IBM Plex Sans Variable',
    );
  });

  afterEach(cleanup);

  it('has the stylesheet: a menu is drawn as one', () => {
    // Without the app's classes every box here is the engine's default, and
    // the rest of this file would be reading a menu the app never draws.
    const menu = openDraw([choice('Default', true)]);
    expect(getComputedStyle(menu).position).toBe('absolute');
    expect(getComputedStyle(menu).minWidth).toBe('200px');
  });

  it('gives each image model one line, under the one before it', () => {
    const menu = openDraw([{ label: NOTE, note: true }, choice('Default', true), ...MODELS.map((label) => choice(label))]);
    expectRowsApart(menu);
    const [standard, ...models] = rowsOf(menu);
    const oneLine = standard.getBoundingClientRect().height;
    // The height of every other small button: a menu's rows are no taller for being able to grow.
    expect(oneLine).toBe(28);
    for (const row of models) {
      expect(linesOf(row), `"${row.textContent}" is on one line`).toHaveLength(1);
      expect(row.getBoundingClientRect().height).toBe(oneLine);
    }
  });

  it('is as wide as its rows, whatever a note says', () => {
    const rows = [choice('Default', true), ...MODELS.map((label) => choice(label))];
    const without = openDraw(rows).getBoundingClientRect().width;
    cleanup();
    const menu = openDraw([{ label: NOTE, note: true }, ...rows]);
    expect(menu.getBoundingClientRect().width).toBe(without);
    const note = menu.querySelector('p')!.getBoundingClientRect();
    expect(note.right).toBeLessThanOrEqual(menu.getBoundingClientRect().right);
    expect(note.height).toBeGreaterThan(32);
  });

  it('grows a row whose label is longer than the menu is wide, each line starting where the first does', () => {
    const menu = openDraw([choice('Default', true), choice(LONG), choice('After it')]);
    expectRowsApart(menu);
    const long = rowsOf(menu)[1];
    const lines = linesOf(long);
    expect(lines.length).toBeGreaterThan(1);
    for (const line of lines) expect(Math.abs(line.left - lines[0].left)).toBeLessThan(0.5);
    const box = menu.getBoundingClientRect();
    expect(box.width).toBe(WIDEST_SUB);
    // The submenu at its widest, beside the menu it opens from, in the room `[+]` has.
    const plus = document.querySelector('[aria-label="Add to the message"]')!.getBoundingClientRect();
    expect(box.right - plus.left).toBeLessThanOrEqual(ROOM);
    expect(long.getBoundingClientRect().right).toBeLessThanOrEqual(box.right);
  });

  it('breaks a name with nowhere to break, inside the menu', () => {
    const menu = openDraw([choice('Default', true), choice('x'.repeat(120)), choice('After it')]);
    expectRowsApart(menu);
    const box = menu.getBoundingClientRect();
    expect(box.width).toBeLessThanOrEqual(WIDEST_SUB);
    for (const line of linesOf(rowsOf(menu)[1])) expect(line.right).toBeLessThanOrEqual(box.right + 0.5);
  });

  it('gives a menu under its own button one line a row too', () => {
    // The model picker's rows: a name and its state.
    const view = render(
      <div style={{ position: 'fixed', right: 40, bottom: 40 }}>
        <Menu label="Qwen3-4B" side="above" items={['Qwen3-Coder-30B-A3B-Instruct · Not loaded', 'Qwen3-4B · Loaded'].map((label) => choice(label))} />
      </div>,
    );
    fireEvent.click(view.getByRole('button', { name: 'Qwen3-4B' }));
    const menu = view.getByRole('menu');
    expectRowsApart(menu);
    for (const row of rowsOf(menu)) expect(linesOf(row)).toHaveLength(1);
  });

  it('is no wider than 24rem under its own button, a longer label wrapped', () => {
    const view = render(
      <div style={{ position: 'fixed', right: 40, bottom: 40 }}>
        <Menu label="Qwen3-4B" side="above" items={[choice(LONG), choice('After it')]} />
      </div>,
    );
    fireEvent.click(view.getByRole('button', { name: 'Qwen3-4B' }));
    const menu = view.getByRole('menu');
    expectRowsApart(menu);
    expect(menu.getBoundingClientRect().width).toBe(WIDEST);
    expect(linesOf(rowsOf(menu)[0]).length).toBeGreaterThan(1);
  });
});
