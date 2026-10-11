/**
 * The hub's chats as a paired device reads them, held to what the daemon
 * sends.
 *
 * `contracts/chats/recorded.json` is written by `gglib-core`'s wire tests
 * from the real serialisation of `GET /v1/chats`, `GET /v1/chats/{id}`, a
 * device's turn (`PUT /v1/runs/{id}?kind=agent`) with and without an image
 * and one that turns thinking off, and the answer to the upload that image
 * was sent by (`POST /v1/attachments`); and of a branch opened, the change
 * that made it (`POST /v1/chats/{id}/changes`) and its answer, and the turn
 * that answers the branch's question (ADR 0017). It fails there when it
 * goes stale. ggchat hand-copies the same file. Here each body must fit the
 * generated types, carry the keys and value types they promise, and leave
 * out what it has no value for (never a `null`, but for the two conversation
 * fields that are always written).
 */

import { describe, it, expect } from 'vitest';

import type { AttachmentUpload } from '../../../src/types/generated/AttachmentUpload';
import type { ChatChange } from '../../../src/types/generated/ChatChange';
import type { ChatChanged } from '../../../src/types/generated/ChatChanged';
import type { HubChatList } from '../../../src/types/generated/HubChatList';
import type { HubChatOpen } from '../../../src/types/generated/HubChatOpen';
import type { HubTurn } from '../../../src/types/generated/HubTurn';
import { turnMadeFromMetadata } from '../../../src/utils/messages/turnMade';
import { rust } from './rustSource';

type Body = Record<string, unknown>;

const RECORDED = JSON.parse(rust('contracts/chats/recorded.json')) as {
  list: HubChatList;
  open: HubChatOpen;
  turn: HubTurn;
  upload: AttachmentUpload;
  image_turn: HubTurn;
  thinking_turn: HubTurn;
  branch_open: HubChatOpen;
  change: ChatChange;
  changed: ChatChanged;
  answer_turn: HubTurn;
};

/** The two keys every turn carries. */
const TURN_KEYS = { conversation_id: 'number', content: 'string' };
/**
 * Every key a turn may carry beside them. Typed from the generated `HubTurn`,
 * so a key added there does not compile here until it is named.
 */
const TURN_OPTIONAL: Record<Exclude<keyof HubTurn, keyof typeof TURN_KEYS>, string> = {
  images: 'object',
  thinking: 'string',
  answer_saved: 'boolean',
  draw: 'boolean',
};

/** What a stored image is told as: its id and what its header says. */
const IMAGE_KEYS = { id: 'string', mime: 'string', width: 'number', height: 'number' };
const IMAGE_ID = /^[0-9a-f]{64}$/;

/** Every key present has the type named, and every required key is present. */
function expectKeys(body: Body, required: Record<string, string>, optional: Record<string, string>) {
  for (const [key, type] of Object.entries(required)) {
    expect(body, key).toHaveProperty(key);
    if (type !== 'nullable-string' && type !== 'nullable-number') expect(typeof body[key], key).toBe(type);
  }
  for (const [key, value] of Object.entries(body)) {
    const type = required[key] ?? optional[key];
    expect(type, `a key the type does not name: ${key}`).toBeDefined();
    if (key in optional) expect(value, `${key} is left out, never null`).not.toBeNull();
    if (type === 'nullable-string' || type === 'nullable-number') {
      if (value !== null) expect(typeof value, key).toBe(type.slice('nullable-'.length));
    } else {
      expect(typeof value, key).toBe(type);
    }
  }
}

describe('the recorded hub chats', () => {
  it('a listing is `{chats}`, each chat its id, title and time, and what else it has', () => {
    expect(Object.keys(RECORDED.list)).toEqual(['chats']);
    expect(RECORDED.list.chats.length).toBeGreaterThan(1);
    for (const chat of RECORDED.list.chats) {
      expectKeys(
        chat as unknown as Body,
        { id: 'number', title: 'string', updated_at: 'string' },
        { model_id: 'number', model: 'string', live_run: 'string', branch_of: 'number' },
      );
    }
    expect(RECORDED.list.chats.some((c) => c.live_run)).toBe(true);
    expect(RECORDED.list.chats.some((c) => !('live_run' in c))).toBe(true);
    expect(RECORDED.list.chats.some((c) => c.branch_of === 12)).toBe(true);
    expect(RECORDED.list.chats.some((c) => !('branch_of' in c))).toBe(true);
  });

  it('an open chat is the conversation and its rows, each with the metadata the hub saved', () => {
    expect(Object.keys(RECORDED.open)).toEqual(['conversation', 'messages']);
    expectKeys(
      RECORDED.open.conversation as unknown as Body,
      {
        id: 'number',
        title: 'string',
        model_id: 'nullable-number',
        system_prompt: 'nullable-string',
        created_at: 'string',
        updated_at: 'string',
      },
      { settings: 'object', branch_of: 'number' },
    );
    expect(RECORDED.open.messages).toHaveLength(4);
    for (const message of RECORDED.open.messages) {
      expectKeys(
        message as unknown as Body,
        { id: 'number', conversation_id: 'number', role: 'string', content: 'string', created_at: 'string' },
        { metadata: 'object', images: 'object' },
      );
      expect(['system', 'user', 'assistant', 'tool']).toContain(message.role);
    }
    const [user, reply] = RECORDED.open.messages;
    expect(user.metadata?.device).toBe('phone-7c2e');
    expect(reply.metadata?.modelName).toBeTypeOf('string');
  });

  it('a finished reply says how it was made, its context among it, and a stopped one says only that it stopped', () => {
    const [, reply, , stopped] = RECORDED.open.messages;
    expect(turnMadeFromMetadata(reply.metadata)).toEqual({
      modelName: 'qwen3-8b',
      promptTokens: 812,
      completionTokens: 96,
      turnDurationMs: 4100,
      device: 'phone-7c2e',
      finishReason: 'stop',
      contextSize: 8192,
    });
    // Every key the hub saved on the reply is one the page reads.
    expect(Object.keys(turnMadeFromMetadata(reply.metadata) ?? {}).sort()).toEqual(
      Object.keys(reply.metadata ?? {}).sort(),
    );
    expect(stopped.role).toBe('assistant');
    expect(stopped.metadata).toEqual({ incomplete: true });
    expect(turnMadeFromMetadata(stopped.metadata)).toBeUndefined();
  });

  it('a row carries its images without their bytes, and a row with none leaves the key out', () => {
    const [user, reply] = RECORDED.open.messages;
    expect(user.images).toHaveLength(1);
    for (const image of user.images ?? []) {
      expectKeys(image as unknown as Body, IMAGE_KEYS, {});
      expect(image.id).toMatch(IMAGE_ID);
    }
    expect(reply).not.toHaveProperty('images');
  });

  it('an upload answers the stored image and the tokens it is estimated to cost', () => {
    expectKeys(RECORDED.upload as unknown as Body, { ...IMAGE_KEYS, image_tokens: 'number' }, {});
    expect(RECORDED.upload.id).toMatch(IMAGE_ID);
  });

  it("a device's turn is the chat's id and the message, and nothing more", () => {
    expectKeys(RECORDED.turn as unknown as Body, TURN_KEYS, {});
    expect(Object.keys(RECORDED.turn)).toEqual(['conversation_id', 'content']);
  });

  it('a turn with an image names it by the id its upload answered, beside the message', () => {
    expectKeys(RECORDED.image_turn as unknown as Body, TURN_KEYS, { images: TURN_OPTIONAL.images });
    expect(RECORDED.image_turn.images).toEqual([RECORDED.upload.id]);
  });

  it('a turn that turns thinking off says so in one word beside the message', () => {
    expectKeys(RECORDED.thinking_turn as unknown as Body, TURN_KEYS, { thinking: TURN_OPTIONAL.thinking });
    expect(RECORDED.thinking_turn).toEqual({
      conversation_id: 12,
      content: 'Answer in one line.',
      thinking: 'off',
    });
  });

  it('a turn carries no key but its two, its images, its thinking choice, whether it answers a saved question and draw', () => {
    expect(Object.keys(TURN_OPTIONAL).sort()).toEqual(['answer_saved', 'draw', 'images', 'thinking']);
    for (const turn of [RECORDED.turn, RECORDED.image_turn, RECORDED.thinking_turn, RECORDED.answer_turn]) {
      expectKeys(turn as unknown as Body, TURN_KEYS, TURN_OPTIONAL);
    }
  });

  it('a branch opened says where its family parts, each option a chat, and that its last question is unanswered', () => {
    expect(Object.keys(RECORDED.branch_open)).toEqual(['conversation', 'messages', 'points', 'answerable']);
    expect(RECORDED.branch_open.conversation).toMatchObject({ id: 13, branch_of: 12 });
    expect(RECORDED.branch_open.conversation).not.toHaveProperty('lineage_id');
    expect(RECORDED.branch_open.answerable).toBe(true);
    const [point] = RECORDED.branch_open.points ?? [];
    expect(point.message_id).toBe(RECORDED.branch_open.messages.at(-1)?.id);
    expect(point.options[point.index].conversation_id).toBe(13);
    for (const option of point.options) {
      expectKeys(option as unknown as Body, { conversation_id: 'number', message_id: 'nullable-number', role: 'nullable-string', preview: 'string' }, {});
    }
  });

  it('a change is the body the page sends its own daemon, answered as the daemon answers it', () => {
    expect(RECORDED.change).toEqual({ kind: 'edit', message_id: 42, content: 'And how do I pin it?' });
    expect(RECORDED.changed).toEqual({ conversation_id: 13, forked: true, answer: true });
  });

  it('the turn that answers a saved question says so, with no message of its own', () => {
    expect(RECORDED.answer_turn).toEqual({ conversation_id: 13, content: '', answer_saved: true });
  });

  it('the open chat remembers that thinking is off, beside its other settings', () => {
    expect(RECORDED.open.conversation.settings).toEqual({ max_iterations: 8, thinking: 'off' });
  });
});
