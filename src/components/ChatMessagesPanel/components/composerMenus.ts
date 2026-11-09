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
import type { ChatTools } from '../../../hooks/useChatTools';
import { componentRoleLabel, imageFamilyLabel } from '../../../utils/imageFamily';
import { folderOffer } from './FolderChip';

/** Tools ▸'s line for a model that cannot call tools. */
export const CANT_CALL_TOOLS = "This model can't call tools";
/** Why nothing is offered while the model is not loaded. */
export const NOT_LOADED = 'The model is not loaded.';
/** Draw ▸'s title while it is pressed. */
export const DRAW_ARMED = 'The next message draws a picture. × on the chip sends it without drawing.';

/**
 * A reason the daemon wrote, as the menu shows it: its words kept, its first
 * letter capitalised, since the machine writes it to follow a colon.
 */
export function asSentence(reason: string | undefined): string | undefined {
  return reason && reason.charAt(0).toUpperCase() + reason.slice(1);
}

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
  const reason = asSentence(draw.reason);
  const choosing = models.length >= 2 || picked !== null;
  if (!draw.available && !choosing) return { label: 'Draw a picture', disabled: true, title: reason };
  if (!choosing) {
    return {
      label: 'Draw a picture',
      onPick: () => !draw.armed && draw.toggle(),
      title: draw.armed ? DRAW_ARMED : drawOffer(draw.model),
    };
  }
  const unlisted = picked !== null && !models.some((row) => row.id === picked);
  const submenu: MenuItem[] = [
    ...(!draw.available && reason ? [{ label: reason, note: true }] : []),
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
  return { label: 'Draw a picture', submenu, title: draw.available ? drawOffer(draw.model) : reason };
}

/** A tool's name as the checklist reads it: `get_current_time` is "Get Current Time". */
export function toolLabel(name: string): string {
  return name
    .split('_')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

/** A row of the checklist: what it says, and the name the daemon knows it by. */
export interface ListedTool {
  label: string;
  title?: string;
  name: string;
}

/** A tool the page's registry holds: its definition, its daemon name, and its server's id (none for a builtin). */
export interface RegisteredTool {
  definition: ToolDefinition;
  name: string;
  server: number | null;
}

/** An enabled tool server, as Settings › Tools lists it. */
export interface EnabledServer {
  id: number;
  name: string;
}

/** Tools ▸'s line while the tool servers are being read. */
export const ASKING = 'Asking gglib';

/** A server whose tools the page has not read, as one row. */
export const wholeServerLabel = (name: string) => `${name} · all its tools`;

/**
 * The checklist's rows: the builtins, then each enabled server, as its
 * tools where the page's registry holds them (a running server whose tools
 * were read) and otherwise as one row for all of them (`<id>:*`), so every
 * enabled server is in the list and unticking from every tool drops none.
 * A server that is not enabled is not offered, even with its tools read.
 */
export function toolRows(registered: readonly RegisteredTool[], servers: readonly EnabledServer[]): ListedTool[] {
  const row = (tool: RegisteredTool): ListedTool => ({
    label: toolLabel(tool.definition.function.name),
    title: tool.definition.function.description || undefined,
    name: tool.name,
  });
  const rows = registered.filter((tool) => tool.server === null).map(row);
  for (const server of servers) {
    const own = registered.filter((tool) => tool.server === server.id);
    rows.push(...(own.length > 0 ? own.map(row) : [{ label: wholeServerLabel(server.name), name: `${server.id}:*` }]));
  }
  return rows;
}

/**
 * Tools ▸, the chat's saved tools as a checklist: none on a far chat, whose
 * tools are its machine's; for a model that cannot call tools, the line
 * that says so; while the tool servers are not read, the line that says
 * why (`listed` a sentence); otherwise Enable all, Disable all and one box
 * per row (`toolRows`), each ticked as the chat saved it (`useChatTools`),
 * and each change saved with the chat.
 */
export function toolsItem(
  listed: readonly ListedTool[] | string,
  chat: ChatTools | undefined,
  supportsToolCalls: boolean | null | undefined,
  toolFormat: string | null | undefined,
): MenuItem | null {
  if (!chat) return null;
  const title = supportsToolCalls && toolFormat ? `This model calls tools (${toolFormat} format).` : undefined;
  if (supportsToolCalls === false) return { label: 'Tools', submenu: [{ label: CANT_CALL_TOOLS, note: true }] };
  if (typeof listed === 'string') return { label: 'Tools', title, submenu: [{ label: listed, note: true }] };
  if (listed.length === 0) return { label: 'Tools', title, submenu: [{ label: 'No tools registered', note: true }] };
  const names = listed.map((tool) => tool.name);
  const submenu: MenuItem[] = [
    { label: 'Enable all', toggles: true, disabled: chat.every, onPick: () => chat.all(true) },
    { label: 'Disable all', toggles: true, disabled: chat.none, onPick: () => chat.all(false) },
    ...listed.map(({ label, title: about, name }, index) => {
      const ticked = chat.ticked(name);
      return {
        label,
        title: about,
        checked: ticked,
        toggles: true,
        gapBefore: index === 0,
        onPick: () => chat.toggle(name, !ticked, names),
      };
    }),
  ];
  return { label: 'Tools', title, submenu };
}
