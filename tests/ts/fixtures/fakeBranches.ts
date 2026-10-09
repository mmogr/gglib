/**
 * gglib's branching rules (`crates/gglib-core/src/domain/branching`), for
 * the fake daemon: which change is made in place, which on a new branch of
 * the chat, and which is refused; and the branch points a chat's family
 * holds along it, each option shown by a line. `tests/ts/contracts/branching.test.ts`
 * holds them to the cases the Rust records in `contracts/chats/branching.json`.
 *
 * A question is one user row; a reply is the run of assistant and tool rows
 * after it.
 */

import type { BranchOption } from '../../../src/types/generated/BranchOption';
import type { BranchPoint } from '../../../src/types/generated/BranchPoint';
import type { ChatChange } from '../../../src/types/generated/ChatChange';

/** A saved message as the rules read it. */
export interface PathRow {
  id: number;
  role: string;
  content: string;
  images?: string[];
}

export type Plan =
  | { replace: { question: number } }
  | { fork: { through: number | null; then: 'nothing' | 'question' | 'edited_reply'; answer: boolean } };

export type Refused = 'message_not_found' | 'unchanged' | 'not_a_reply' | 'nothing_to_answer' | 'invalid_request';

/** What the rules answer a change with. */
export type Planned = { plan: Plan } | { refused: Refused };

/** The status each refusal is answered with. */
export const REFUSED_STATUS: Record<Refused, number> = {
  message_not_found: 404,
  unchanged: 400,
  not_a_reply: 400,
  invalid_request: 400,
  nothing_to_answer: 409,
};

/** A turn: the rows `[start, end)` of a question (one row) or a reply. */
interface Unit {
  question: boolean;
  start: number;
  end: number;
}

function units(path: PathRow[]): Unit[] {
  const out: Unit[] = [];
  path.forEach((row, at) => {
    if (row.role === 'user') out.push({ question: true, start: at, end: at + 1 });
    else if (row.role === 'assistant' || row.role === 'tool') {
      const last = out[out.length - 1];
      if (last && !last.question) last.end = at + 1;
      else out.push({ question: false, start: at, end: at + 1 });
    }
  });
  return out;
}

const sameImages = (a: string[] = [], b: string[] = []) => a.length === b.length && a.every((id, i) => id === b[i]);

/** What `change` writes to a chat whose messages are `path`, `busy` saying whether a reply to it is being written. */
export function plan(path: PathRow[], change: ChatChange, busy: boolean): Planned {
  const at = path.findIndex((row) => row.id === change.message_id);
  const turns = units(path);
  const held = turns.findIndex((unit) => at >= unit.start && at < unit.end);
  if (at === -1 || held === -1) return { refused: 'message_not_found' };
  const unit = turns[held];
  const before = (start: number) => (start > 0 ? path[start - 1].id : null);
  if (change.kind === 'branch') {
    return { plan: { fork: { through: path[unit.end - 1].id, then: 'nothing', answer: false } } };
  }
  if (change.kind === 'regenerate') {
    if (unit.question) return { refused: 'not_a_reply' };
    if (held === 0 || !turns[held - 1].question) return { refused: 'nothing_to_answer' };
    return { plan: { fork: { through: before(unit.start), then: 'nothing', answer: true } } };
  }
  const images = change.images ?? [];
  if (!unit.question) {
    if (images.length > 0) return { refused: 'invalid_request' };
    if (change.content === path[at].content) return { refused: 'unchanged' };
    return { plan: { fork: { through: before(unit.start), then: 'edited_reply', answer: false } } };
  }
  if (change.content === path[at].content && sameImages(path[at].images, images)) return { refused: 'unchanged' };
  if (held === turns.length - 1 && !busy) return { plan: { replace: { question: change.message_id } } };
  return { plan: { fork: { through: before(unit.start), then: 'question', answer: true } } };
}

/** Whether a chat whose messages are `path` ends in a question with no reply. */
export function answerable(path: PathRow[]): boolean {
  return units(path).at(-1)?.question ?? false;
}

/** A message of a chat of the family, as the points read it: `key` is the message it copies as first written, or its own id. */
export interface LineRow {
  id: number;
  key: number;
  role: string;
  text: string;
  images: number;
}

/** A chat of the family, and when it last changed. */
export interface LineChat {
  conversation_id: number;
  updated_at: string;
  rows: LineRow[];
}

/** The longest preview, in characters, before it is cut with "…". */
const PREVIEW_CHARS = 80;

function firstLine(text: string): string {
  const line = text.split(/\r?\n/).map((l) => l.trim()).find((l) => l !== '') ?? '';
  const chars = [...line];
  return chars.length > PREVIEW_CHARS ? `${chars.slice(0, PREVIEW_CHARS).join('')}…` : line;
}

/** The line a turn is shown by among the options of a point. */
export function preview(turn: Array<Pick<LineRow, 'role' | 'text' | 'images'>>): string {
  const [first] = turn;
  if (!first) return '';
  if (first.role === 'user') {
    const line = firstLine(first.text);
    if (line !== '' || first.images === 0) return line;
    return first.images === 1 ? 'An image' : `${first.images} images`;
  }
  const lines = turn.filter((r) => r.role === 'assistant').map((r) => firstLine(r.text)).reverse();
  return lines.find((line) => line !== '') ?? '(no text)';
}

const startsTurn = (rows: LineRow[], at: number) =>
  at < rows.length && (rows[at].role === 'user' || at === 0 || rows[at - 1].role === 'user');

const rank = (c: LineChat) => [c.updated_at, c.conversation_id] as const;
const newer = (a: LineChat, b: LineChat) =>
  rank(a)[0] > rank(b)[0] || (rank(a)[0] === rank(b)[0] && rank(a)[1] > rank(b)[1]);

function option(chat: LineChat, at: number): BranchOption {
  const row = chat.rows[at];
  const turn: LineRow[] = [row];
  if (row.role !== 'user') {
    for (const next of chat.rows.slice(at + 1)) {
      if (next.role === 'user') break;
      turn.push(next);
    }
  }
  return { conversation_id: chat.conversation_id, message_id: row.id, role: row.role as BranchOption['role'], preview: preview(turn) };
}

function point(mine: LineChat, family: LineChat[], at: number): BranchPoint | null {
  const shares = (chat: LineChat) => at === 0 || chat.rows[at - 1]?.key === mine.rows[at - 1].key;
  const turns = new Map<number, LineChat>();
  for (const chat of family.filter(shares)) {
    if (!startsTurn(chat.rows, at)) continue;
    const key = chat.rows[at].key;
    const shown = turns.get(key);
    if (!shown || (shown.conversation_id !== mine.conversation_id && (chat.conversation_id === mine.conversation_id || newer(chat, shown)))) {
      turns.set(key, chat);
    }
  }
  const empty = at === mine.rows.length;
  if (turns.size + (empty ? 1 : 0) < 2) return null;
  const options = [...turns.entries()].sort(([a], [b]) => a - b).map(([, chat]) => option(chat, at));
  if (empty) options.push({ conversation_id: mine.conversation_id, message_id: null, role: null, preview: '' });
  const index = options.findIndex((o) => o.conversation_id === mine.conversation_id);
  if (index === -1) return null;
  return { message_id: mine.rows[at]?.id ?? null, index, options };
}

/** The branch points chat `me`'s family holds along it. */
export function points(me: number, family: LineChat[]): BranchPoint[] {
  const mine = family.find((chat) => chat.conversation_id === me);
  if (!mine) return [];
  const starts = units(mine.rows.map((r) => ({ id: r.id, role: r.role, content: r.text }))).map((u) => u.start);
  return [...starts, mine.rows.length].flatMap((at) => point(mine, family, at) ?? []);
}
