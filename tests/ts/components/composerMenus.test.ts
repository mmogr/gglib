/**
 * The composer's `[+]` menu, item by item, from the facts handed in: each
 * item disabled with its reason where it cannot be used, Draw a picture a
 * submenu only where there is a model to choose, and Tools ▸ either the
 * checklist or the line that says the model cannot call tools. The menu as
 * drawn is `tests/ts/places/chat/ChatPlaceComposer.test.tsx`'s.
 */

import { describe, it, expect, vi } from 'vitest';
import {
  CANT_CALL_TOOLS,
  drawItem,
  folderItem,
  imageItem,
  toolLabel,
  toolRows,
  toolsItem,
  type ListedTool,
  type RegisteredTool,
} from '../../../src/components/ChatMessagesPanel/components/composerMenus';
import type { ChatFolder } from '../../../src/hooks/useChatFolder';
import type { DrawSwitch } from '../../../src/hooks/useDrawSwitch';
import type { ChatTools } from '../../../src/hooks/useChatTools';
import type { ImageModelRow } from '../../../src/types/generated/ImageModelRow';

const folder = (over: Partial<ChatFolder> = {}): ChatFolder => ({
  shown: true,
  path: null,
  error: null,
  native: true,
  sentTo: null,
  unread: false,
  pick: vi.fn(async () => {}),
  choose: vi.fn(),
  clear: vi.fn(),
  forSend: () => undefined,
  ...over,
});

const row = (id: number, name: string, over: Partial<ImageModelRow> = {}): ImageModelRow =>
  ({ id, name, family: 'flux1', resident: false, complete: true, missing: [], default: false, ...over }) as ImageModelRow;

const draw = (over: Partial<DrawSwitch> = {}): DrawSwitch => ({
  available: true,
  model: 'flux-schnell',
  armed: false,
  toggle: vi.fn(),
  forSend: () => undefined,
  models: [],
  picked: null,
  pick: vi.fn(),
  ...over,
});

/** The registry's tool `name` of server `server` (a builtin with none). */
const registered = (name: string, server: number | null = null): RegisteredTool => ({
  definition: {
    type: 'function' as const,
    function: { name, description: `does ${name}`, parameters: { type: 'object' as const, properties: {} } },
  },
  name: server === null ? `builtin:${name}` : `${server}:${name}`,
  server,
});

/** The rows of the builtins named. */
const listed = (names: string[]): ListedTool[] => toolRows(names.map((name) => registered(name)), []);

/** The chat's saved tools: `ticked` by daemon name, every one or none. */
const chat = (ticked: string[], over: Partial<ChatTools> = {}): ChatTools => ({
  ticked: (name) => ticked.includes(name),
  every: false,
  none: false,
  toggle: vi.fn(),
  all: vi.fn(),
  forCreate: () => undefined,
  ...over,
});

describe('imageItem', () => {
  it('is offered for a model that reads images, loaded or not, and adds one when picked', () => {
    const add = vi.fn();
    const item = imageItem({ offered: true, reason: null, contextLength: 4096 }, add);
    expect(item).toMatchObject({ label: 'Add image…', disabled: false, title: undefined });
    item.onPick!();
    expect(add).toHaveBeenCalledTimes(1);
  });

  it("is greyed with the model's reason", () => {
    expect(imageItem({ offered: false, reason: 'no projector', contextLength: null }, vi.fn())).toMatchObject({ disabled: true, title: 'no projector' });
  });
});

describe('folderItem', () => {
  it('is none for a far chat, or without a folder', () => {
    expect(folderItem(folder(), true, vi.fn())).toBeNull();
    expect(folderItem(undefined, false, vi.fn())).toBeNull();
  });

  it('is greyed for a model that calls no tools, and otherwise chooses, saying where', () => {
    expect(folderItem(folder({ shown: false }), false, vi.fn())).toMatchObject({
      disabled: true,
      title: `${CANT_CALL_TOOLS}, so nothing would read a folder.`,
    });
    const change = vi.fn();
    const item = folderItem(folder(), false, change)!;
    expect(item.disabled).toBeFalsy();
    expect(item.title).toMatch(/^Choose a folder on this machine/);
    item.onPick!();
    expect(change).toHaveBeenCalledTimes(1);
  });
});

describe('drawItem', () => {
  it("is greyed with the machine's reason, its first letter capitalised, where a message cannot draw and nothing is chosen", () => {
    expect(drawItem(draw({ available: false, reason: 'no image model' }))).toEqual({ label: 'Draw a picture', disabled: true, title: 'No image model' });
  });

  it('with fewer than two models, is one action that sets Draw, and never unsets it', () => {
    const unset = draw();
    const item = drawItem(unset)!;
    expect(item.submenu).toBeUndefined();
    item.onPick!();
    expect(unset.toggle).toHaveBeenCalledTimes(1);

    const set = draw({ armed: true, models: [row(3, 'flux-schnell')] });
    drawItem(set)!.onPick!();
    expect(set.toggle).not.toHaveBeenCalled();
  });

  it('with two or more models, is a submenu: the default rule, then each, the chosen one ticked and one lacking a file greyed', () => {
    const many = draw({
      models: [row(3, 'flux-schnell', { default: true, resident: true }), row(7, 'sdxl-base', { complete: false, missing: ['vae'] })],
      picked: 3,
    });
    const sub = drawItem(many)!.submenu!;
    expect(sub.map((item) => [item.label, item.checked, item.disabled ?? false])).toEqual([
      ['Default (flux-schnell)', false, false],
      ['flux-schnell · Flux.1 · loaded', true, false],
      ['sdxl-base · Flux.1 · incomplete', false, true],
    ]);
    expect(sub[2].title).toBe('Missing: VAE');
    sub[0].onPick!();
    expect(many.pick).toHaveBeenCalledWith(null);
  });

  it('keeps the submenu while a choice stands, though nothing can draw, with the reason above it', () => {
    const stuck = draw({ available: false, reason: 'gone; choose another', models: [], picked: 7 });
    const item = drawItem(stuck)!;
    expect(item.disabled).toBeFalsy();
    expect(item.submenu![0]).toEqual({ label: 'Gone; choose another', note: true });
    expect(item.submenu!.find((i) => i.label === 'Model 7 (no longer listed)')).toMatchObject({ checked: true });
  });
});

describe('toolsItem', () => {
  it('is not offered on a far chat, whose tools are its machine\'s', () => {
    expect(toolsItem(listed(['read_file']), undefined, true, null)).toBeNull();
  });

  it("says the model can't call tools, and offers no checklist, for a model that cannot", () => {
    const sub = toolsItem(listed(['read_file']), chat([]), false, null)!.submenu!;
    expect(sub).toEqual([{ label: "This model can't call tools", note: true }]);
    expect(CANT_CALL_TOOLS).toBe("This model can't call tools");
  });

  it('is the checklist where whether it can is not yet known, and names the format where it can', () => {
    expect(toolsItem(listed(['read_file']), chat([]), null, null)!.submenu!.map((item) => item.label)).toEqual([
      'Enable all',
      'Disable all',
      'Read File',
    ]);
    expect(toolsItem(listed(['read_file']), chat([]), true, 'hermes')!.title).toBe('This model calls tools (hermes format).');
    expect(toolsItem(listed(['read_file']), chat([]), null, 'hermes')!.title).toBeUndefined();
  });

  it('ticks each tool as the chat saved it, and a pick toggles it by its daemon name among every listed tool', () => {
    const saved = chat(['builtin:read_file']);
    const [enable, disable, time, read] = toolsItem(listed(['get_current_time', 'read_file']), saved, true, null)!.submenu!;
    expect([enable.disabled, disable.disabled]).toEqual([false, false]);
    expect([time.checked, read.checked, time.toggles, read.toggles]).toEqual([false, true, true, true]);
    time.onPick!();
    expect(saved.toggle).toHaveBeenLastCalledWith('builtin:get_current_time', true, ['builtin:get_current_time', 'builtin:read_file']);
    read.onPick!();
    expect(saved.toggle).toHaveBeenLastCalledWith('builtin:read_file', false, ['builtin:get_current_time', 'builtin:read_file']);
    enable.onPick!();
    expect(saved.all).toHaveBeenLastCalledWith(true);
    disable.onPick!();
    expect(saved.all).toHaveBeenLastCalledWith(false);
  });

  it('greys Enable all where every tool is, and Disable all where none is', () => {
    const every = toolsItem(listed(['a', 'b']), chat(['builtin:a', 'builtin:b'], { every: true }), true, null)!.submenu!;
    expect([every[0].disabled, every[1].disabled]).toEqual([true, false]);
    const none = toolsItem(listed(['a', 'b']), chat([], { none: true }), true, null)!.submenu!;
    expect([none[0].disabled, none[1].disabled]).toEqual([false, true]);
  });

  it('says why there is no checklist while the tool servers are not read', () => {
    expect(toolsItem('Asking gglib', chat([]), true, null)!.submenu).toEqual([{ label: 'Asking gglib', note: true }]);
  });

  it('says so where no tool is registered', () => {
    const sub = toolsItem([], chat([]), true, null)!.submenu!;
    expect(sub).toEqual([{ label: 'No tools registered', note: true }]);
  });

  it('lists the builtins, then each enabled server: its tools where they were read, else one row for all of them', () => {
    const rows = toolRows(
      [registered('read_file'), registered('search', 3), registered('extract', 3), registered('crawl', 9)],
      [{ id: 3, name: 'web' }, { id: 4, name: 'github' }],
    );
    expect(rows.map((row) => [row.label, row.name])).toEqual([
      ['Read File', 'builtin:read_file'],
      ['Search', '3:search'],
      ['Extract', '3:extract'],
      ['github · all its tools', '4:*'],
    ]);
    expect(rows[1].title).toBe('does search');
    // Server 9 is not enabled: its tools are not offered, though they were read.
    expect(rows.some((row) => row.name.startsWith('9:'))).toBe(false);
  });

  it('names a tool as words: get_current_time is "Get Current Time"', () => {
    expect(toolLabel('get_current_time')).toBe('Get Current Time');
  });
});
