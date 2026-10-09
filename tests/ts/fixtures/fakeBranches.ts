/**
 * gglib's branching rules (`crates/gglib-core/src/domain/branching`), for
 * the fake daemon: which change is made in place, which on a new branch of
 * the chat, and which is refused. `tests/ts/contracts/branching.test.ts`
 * holds them to the cases the Rust records in `contracts/chats/branching.json`.
 *
 * A question is one user row; a reply is the run of assistant and tool rows
 * after it.
 */

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
