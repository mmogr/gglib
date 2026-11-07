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
  NOT_LOADED,
  drawItem,
  folderItem,
  imageItem,
  toolLabel,
  toolsItem,
  type ToolsChecklist,
} from '../../../src/components/ChatMessagesPanel/components/composerMenus';
import type { ChatFolder } from '../../../src/hooks/useChatFolder';
import type { DrawSwitch } from '../../../src/hooks/useDrawSwitch';
import type { ImageModelRow } from '../../../src/types/generated/ImageModelRow';
import { loadingReplyOf } from '../../../src/components/ChatMessagesPanel/hooks/useModelLoadWait';

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

const checklist = (names: string[], enabled: string[] = []) => ({
  tools: names.map((name) => ({
    type: 'function' as const,
    function: { name, description: `does ${name}`, parameters: { type: 'object' as const, properties: {} } },
  })),
  isEnabled: (name: string) => enabled.includes(name),
  set: vi.fn((_names: string[], _on: boolean) => {}),
}) satisfies ToolsChecklist;

describe('imageItem', () => {
  it('is offered for a model that reads images while it is loaded, and adds one when picked', () => {
    const add = vi.fn();
    const item = imageItem({ offered: true, reason: null, contextLength: 4096 }, true, add);
    expect(item).toMatchObject({ label: 'Add image…', disabled: false, title: undefined });
    item.onPick!();
    expect(add).toHaveBeenCalledTimes(1);
  });

  it("is greyed with the model's reason, and while nothing serves the chat with that", () => {
    expect(imageItem({ offered: false, reason: 'no projector', contextLength: null }, true, vi.fn())).toMatchObject({ disabled: true, title: 'no projector' });
    expect(imageItem({ offered: true, reason: null, contextLength: null }, false, vi.fn())).toMatchObject({ disabled: true, title: NOT_LOADED });
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
  it("is greyed with the machine's reason where a message cannot draw and nothing is chosen", () => {
    expect(drawItem(draw({ available: false, reason: 'no image model' }))).toEqual({ label: 'Draw a picture', disabled: true, title: 'no image model' });
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
    expect(item.submenu![0]).toEqual({ label: 'gone; choose another', note: true });
    expect(item.submenu!.find((i) => i.label === 'Model 7 (no longer listed)')).toMatchObject({ checked: true });
  });
});

describe('toolsItem', () => {
  it("says the model can't call tools, and offers no checklist, for a model that cannot", () => {
    const sub = toolsItem(checklist(['read_file']), false, null, vi.fn()).submenu!;
    expect(sub.map((item) => item.label)).toEqual([CANT_CALL_TOOLS, 'Agent limits…']);
    expect(sub[0].note).toBe(true);
  });

  it('is the checklist where whether it can is not yet known, and names the format where it can', () => {
    expect(toolsItem(checklist(['read_file']), null, null, vi.fn()).submenu!.map((item) => item.label)).toEqual([
      'Enable all',
      'Disable all',
      'Read File',
      'Agent limits…',
    ]);
    expect(toolsItem(checklist(['read_file']), true, 'hermes', vi.fn()).title).toBe('This model calls tools (hermes format).');
    expect(toolsItem(checklist(['read_file']), null, 'hermes', vi.fn()).title).toBeUndefined();
  });

  it('ticks each tool on and off, leaving the menu open, and greys Enable all or Disable all where it would do nothing', () => {
    const list = checklist(['get_current_time', 'read_file'], ['read_file']);
    const [enable, disable, time, read] = toolsItem(list, true, null, vi.fn()).submenu!;
    expect([enable.disabled, disable.disabled]).toEqual([false, false]);
    expect([time.checked, read.checked, time.toggles, read.toggles]).toEqual([false, true, true, true]);
    time.onPick!();
    expect(list.set).toHaveBeenLastCalledWith(['get_current_time'], true);
    read.onPick!();
    expect(list.set).toHaveBeenLastCalledWith(['read_file'], false);
    enable.onPick!();
    expect(list.set).toHaveBeenLastCalledWith(['get_current_time', 'read_file'], true);

    const all = toolsItem(checklist(['a', 'b'], ['a', 'b']), true, null, vi.fn()).submenu!;
    expect([all[0].disabled, all[1].disabled]).toEqual([true, false]);
    const none = toolsItem(checklist(['a', 'b']), true, null, vi.fn()).submenu!;
    expect([none[0].disabled, none[1].disabled]).toEqual([false, true]);
  });

  it('says so where no tool is registered, and always ends with Agent limits…', () => {
    const open = vi.fn();
    const sub = toolsItem(checklist([]), true, null, open).submenu!;
    expect(sub.map((item) => item.label)).toEqual(['No tools registered', 'Agent limits…']);
    sub[1].onPick!();
    expect(open).toHaveBeenCalledTimes(1);
  });

  it('names a tool as words: get_current_time is "Get Current Time"', () => {
    expect(toolLabel('get_current_time')).toBe('Get Current Time');
  });
});

describe('loadingReplyOf', () => {
  const reply = (status: string, waiting?: object) => ({
    id: 'r1',
    role: 'assistant',
    status: { type: status },
    metadata: { custom: waiting ? { waiting } : {} },
  });

  it('is the newest reply while it waits for its model to load, and nothing otherwise', () => {
    const asked = (messages: unknown[]) => loadingReplyOf(messages as Parameters<typeof loadingReplyOf>[0]);
    expect(asked([reply('running', { reason: 'model_load', step: 0, total: 0, position: 0 })])).toBe('r1');
    expect(asked([reply('running', { reason: 'image_render', step: 1, total: 20, position: 1 })])).toBeNull();
    expect(asked([reply('complete', { reason: 'model_load', step: 0, total: 0, position: 0 })])).toBeNull();
    expect(asked([reply('running')])).toBeNull();
    expect(asked([{ id: 'u', role: 'user', metadata: {} }])).toBeNull();
    expect(asked([])).toBeNull();
  });
});
