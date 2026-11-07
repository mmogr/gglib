/**
 * The composer's `[+]` menu, item by item, from the facts the chat's hooks
 * give: pure, so each item and its disabled reason is read from what it is
 * handed and nothing else.
 *
 * @module ChatMessagesPanel/components/composerMenus
 */

import type { MenuItem } from '../../ui/Menu';
import type { ChatFolder } from '../../../hooks/useChatFolder';
import type { DrawSwitch } from '../../../hooks/useDrawSwitch';
import type { ImageInput } from '../../../hooks/useImageInput';
import type { ImageModelRow } from '../../../types/generated/ImageModelRow';
import type { ToolDefinition } from '../../../services/tools';
import { componentRoleLabel, imageFamilyLabel } from '../../../utils/imageFamily';
import { folderOffer } from './FolderChip';

/** Tools ▸'s line for a model that cannot call tools. */
export const CANT_CALL_TOOLS = "This model can't call tools";
/** Why nothing is offered while the model is not loaded. */
export const NOT_LOADED = 'The model is not loaded.';
/** Draw ▸'s title while it is pressed. */
export const DRAW_ARMED = 'The next message draws a picture. × on the chip sends it without drawing.';

/** What drawing would do, naming the image model when its machine did. */
export function drawOffer(model: string | undefined): string {
  return `Let the next message draw a picture${model ? ` with ${model}` : ''}. It applies to that message only.`;
}

/** "Add image…": disabled with the reason where the model reads no images, or nothing serves it. */
export function imageItem(input: ImageInput, connected: boolean, add: () => void): MenuItem {
  const reason = !input.offered ? input.reason : connected ? null : NOT_LOADED;
  return { label: 'Add image…', onPick: add, disabled: reason !== null, title: reason ?? undefined };
}

/**
 * "Use a folder…": none for a far chat, which has no folder here; disabled
 * for a model that calls no tools, since nothing would read it.
 */
export function folderItem(folder: ChatFolder | undefined, far: boolean, change: () => void): MenuItem | null {
  if (!folder || far) return null;
  if (!folder.shown) return { label: 'Use a folder…', disabled: true, title: `${CANT_CALL_TOOLS}, so nothing would read a folder.` };
  return { label: 'Use a folder…', onPick: change, title: folderOffer(folder) };
}

/** The option that stands for the machine's default rule, as the submenu names it. */
function defaultLabel(models: ImageModelRow[]): string {
  const standing = models.find((row) => row.default);
  return standing ? `Default (${standing.name})` : 'Default';
}

/** A row as the submenu reads it: its name, its family and, when it is, that it is loaded. */
export function imageModelLabel(row: ImageModelRow): string {
  // A family this page does not know is shown as the machine sent it.
  const family = imageFamilyLabel(row.family) ?? row.family;
  return [row.name, family, row.resident && 'loaded', !row.complete && 'incomplete'].filter(Boolean).join(' · ');
}

/** Why a row cannot be chosen: the files it lacks, by role. */
export function missingTitle(row: ImageModelRow): string | undefined {
  if (row.complete) return undefined;
  // A role this page does not know is shown as the machine sent it.
  return `Missing: ${row.missing.map((role) => componentRoleLabel(role) ?? role).join(', ')}`;
}

/**
 * "Draw a picture": a submenu of image models where the machine lists two
 * or more, or the chat has chosen one (so the choice can always be taken
 * back, even once it no longer draws); otherwise one action that presses
 * Draw for the next message. Disabled with the machine's reason where a
 * message cannot draw and no choice stands to change.
 */
export function drawItem(draw: DrawSwitch | undefined): MenuItem | null {
  if (!draw) return null;
  const { models, picked, pick } = draw;
  const choosing = models.length >= 2 || picked !== null;
  if (!draw.available && !choosing) return { label: 'Draw a picture', disabled: true, title: draw.reason };
  if (!choosing) {
    return {
      label: 'Draw a picture',
      onPick: () => !draw.armed && draw.toggle(),
      title: draw.armed ? DRAW_ARMED : drawOffer(draw.model),
    };
  }
  const unlisted = picked !== null && !models.some((row) => row.id === picked);
  const submenu: MenuItem[] = [
    ...(!draw.available && draw.reason ? [{ label: draw.reason, note: true }] : []),
    { label: defaultLabel(models), checked: picked === null, onPick: () => pick(null) },
    ...(unlisted ? [{ label: `Model ${picked} (no longer listed)`, checked: true, onPick: () => pick(picked) }] : []),
    ...models.map((row) => ({
      label: imageModelLabel(row),
      checked: row.id === picked,
      disabled: !row.complete,
      title: missingTitle(row),
      onPick: () => pick(row.id),
    })),
  ];
  return { label: 'Draw a picture', submenu, title: draw.available ? drawOffer(draw.model) : draw.reason };
}

/** A tool's name as the checklist reads it: `get_current_time` is "Get Current Time". */
export function toolLabel(name: string): string {
  return name
    .split('_')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

/** What the tools registry is asked and told, for the checklist. */
export interface ToolsChecklist {
  tools: ToolDefinition[];
  isEnabled: (name: string) => boolean;
  /** Switch the named tools on or off. */
  set: (names: string[], on: boolean) => void;
}

/**
 * Tools ▸: for a model that cannot call tools, the line that says so;
 * otherwise Enable all, Disable all and one box per tool the registry
 * holds, built-in and MCP alike. "Agent limits…" ends it either way.
 */
export function toolsItem(
  list: ToolsChecklist,
  supportsToolCalls: boolean | null | undefined,
  toolFormat: string | null | undefined,
  openLimits: () => void,
): MenuItem {
  const limits: MenuItem = { label: 'Agent limits…', onPick: openLimits, gapBefore: true };
  const title = supportsToolCalls && toolFormat ? `This model calls tools (${toolFormat} format).` : undefined;
  if (supportsToolCalls === false) return { label: 'Tools', submenu: [{ label: CANT_CALL_TOOLS, note: true }, limits] };
  const names = list.tools.map((tool) => tool.function.name);
  if (names.length === 0) return { label: 'Tools', title, submenu: [{ label: 'No tools registered', note: true }, limits] };
  const on = names.filter(list.isEnabled);
  const submenu: MenuItem[] = [
    { label: 'Enable all', toggles: true, disabled: on.length === names.length, onPick: () => list.set(names, true) },
    { label: 'Disable all', toggles: true, disabled: on.length === 0, onPick: () => list.set(names, false) },
    ...list.tools.map((tool, index) => {
      const name = tool.function.name;
      const enabled = list.isEnabled(name);
      return {
        label: toolLabel(name),
        title: tool.function.description || undefined,
        checked: enabled,
        toggles: true,
        gapBefore: index === 0,
        onPick: () => list.set([name], !enabled),
      };
    }),
    limits,
  ];
  return { label: 'Tools', title, submenu };
}
