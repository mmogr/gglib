/**
 * The remote registry: what each event proves, and nothing more.
 *
 * The events are thin on purpose (a fingerprint, a port), so an arm that
 * wrote more than its event carries would be inventing state. And the
 * use-for-chat choice is a preference for a machine; when the machine goes,
 * so does the preference — otherwise the next send goes somewhere the person
 * did not mean.
 *
 * The model name is the same argument one step further in: it is a preference
 * for a machine too, because it is a name in that machine's catalog and
 * nobody else's.
 */

import { describe, it, expect, beforeEach } from 'vitest';

import {
  IDLE_STATUS,
  applyRemoteStatus,
  getRemoteState,
  ingestRemoteEvent,
  requestRemoteChat,
  resetRemoteState,
  setRemoteChatModel,
  setUseRemoteForChat,
} from '../../../src/services/remoteRegistry';

const connected = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: '3ca82708b995',
  path: 'direct',
  // Here rather than away: everything below is about which machine the
  // chat fields follow, and a machine that is answering is the case those
  // rules are written for. The away arm has its own tests.
  away_for_s: null,
};

/** A second machine: the same tunnel, a different catalog. */
const otherMachine = { ...connected, port: 41235, ticket_fingerprint: 'ffee11223344' };

describe('remoteRegistry', () => {
  beforeEach(() => resetRemoteState());

  it('starts with no status and no chat preference', () => {
    expect(getRemoteState()).toEqual({
      status: null,
      useForChat: false,
      chatModel: '',
      chatModelPeer: null,
      chatRequestedAt: null,
    });
  });

  it('remote_enabled turns the serve side on with the fingerprint and a live code', () => {
    ingestRemoteEvent({ type: 'remote_enabled', ticketFingerprint: 'aabbccddeeff' });
    const { status } = getRemoteState();
    expect(status?.enabled).toBe(true);
    expect(status?.ticket_fingerprint).toBe('aabbccddeeff');
    expect(status?.pairing_active).toBe(true);
    expect(status?.paired).toBe(false);
  });

  it('remote_paired spends the code; remote_disabled clears the session', () => {
    ingestRemoteEvent({ type: 'remote_enabled', ticketFingerprint: 'aabbccddeeff' });
    ingestRemoteEvent({ type: 'remote_paired', peer: '0123456789ab' });
    expect(getRemoteState().status).toMatchObject({
      pairing_active: false,
      paired: true,
      last_peer: '0123456789ab',
    });

    ingestRemoteEvent({ type: 'remote_disabled' });
    expect(getRemoteState().status).toMatchObject({
      enabled: false,
      ticket_fingerprint: null,
      pairing_active: false,
      paired: false,
      peers: [],
    });
  });

  it('remote_connected writes a placeholder connection the status read replaces', () => {
    ingestRemoteEvent({ type: 'remote_connected', port: 41234 });
    expect(getRemoteState().status?.connected).toMatchObject({
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
    });

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(getRemoteState().status?.connected).toEqual(connected);
  });

  it('use-for-chat needs a connection and does not outlive it', () => {
    setUseRemoteForChat(true);
    expect(getRemoteState().useForChat).toBe(false);

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);
    expect(getRemoteState().useForChat).toBe(true);

    ingestRemoteEvent({ type: 'remote_disconnected' });
    expect(getRemoteState()).toMatchObject({ useForChat: false });
    expect(getRemoteState().status?.connected).toBeNull();
  });

  it('the model named for that machine outlives a disconnection; the preference does not', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');
    setUseRemoteForChat(true);

    ingestRemoteEvent({ type: 'remote_disconnected' });
    // Nothing can be sent while the box is off, so keeping the name costs
    // nothing — and the ordinary reconnection is the same machine again.
    expect(getRemoteState()).toMatchObject({ useForChat: false, chatModel: 'qwen3' });

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(getRemoteState().chatModel).toBe('qwen3');
  });

  it('the name survives the whole reconnection, event by event', () => {
    // The same promise as above, walked the way production walks it: every
    // event is followed by a status read, and the dial back raises
    // `remote_connected` in between. That arm clears the routing choice, and
    // the name has to be visibly exempt — clearing it there would empty the
    // field on every ordinary reconnection with the suite still green.
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');

    ingestRemoteEvent({ type: 'remote_disconnected' });
    applyRemoteStatus(IDLE_STATUS);
    ingestRemoteEvent({ type: 'remote_connected', port: 41234 });
    expect(getRemoteState().chatModel).toBe('qwen3');

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(getRemoteState()).toMatchObject({ chatModel: 'qwen3', chatModelPeer: '3ca82708b995' });
  });

  it('a name typed for one machine is not offered to a different one', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');

    applyRemoteStatus(IDLE_STATUS);
    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });

    // `qwen3` is a name in the first machine's catalog. Left in the field it
    // would be sent to a machine that never claimed it, and come back
    // `404 Model 'qwen3' not found` from a tunnel that is working.
    expect(getRemoteState().chatModel).toBe('');
    expect(getRemoteState().chatModelPeer).toBe('ffee11223344');
  });

  it('a status read with nobody on the other end keeps the name', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');

    // The production disconnection is the event *and* the status read that
    // follows it, and it is the read that could clear the name. A
    // disconnection is not a different machine, so it must not.
    applyRemoteStatus(IDLE_STATUS);
    expect(getRemoteState()).toMatchObject({ chatModel: 'qwen3', chatModelPeer: '3ca82708b995' });
  });

  it('a name typed before any connection is adopted by the machine that answers', () => {
    // Nothing is connected, so nothing owns the name yet. Whoever answers is
    // who it was typed for.
    setRemoteChatModel('qwen3');
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(getRemoteState().chatModel).toBe('qwen3');

    // Adopted, not unowned: it is that machine's name from here on.
    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(getRemoteState().chatModel).toBe('');
  });

  it('the placeholder connection names no peer, so a name typed against it is adopted', () => {
    // The product's own path to an unattributed name, and the only one that
    // reaches it with something typed. Dialling a second machine: the event
    // lands, the panel flips to the connected view, and the model field is on
    // screen before any status read has said who answered.
    applyRemoteStatus({ ...IDLE_STATUS, connected, stored_ticket_fingerprint: '3ca82708b995' });
    ingestRemoteEvent({ type: 'remote_disconnected' });
    ingestRemoteEvent({ type: 'remote_connected', port: 41235 });
    expect(getRemoteState().status?.connected?.ticket_fingerprint).toBe('');

    // Typed for the machine being dialled, which is not the one whose ticket
    // is stored. Stamping the stored fingerprint here would attribute it to
    // the machine dialled *last*, and the status read would then wipe it.
    setRemoteChatModel('llama3');
    expect(getRemoteState().chatModelPeer).toBeNull();

    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(getRemoteState()).toMatchObject({ chatModel: 'llama3', chatModelPeer: 'ffee11223344' });
  });

  it('a peer swap with no disconnection in between takes the routing choice too', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');
    setUseRemoteForChat(true);
    requestRemoteChat();

    // An SSE gap can drop `remote_disconnected` and leave a status read as
    // the first news of a different machine. The choice and the pending
    // request were made about the old one, so neither transfers.
    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(getRemoteState()).toMatchObject({
      useForChat: false,
      chatModel: '',
      chatRequestedAt: null,
    });
  });

  it('a dial that arrives without its disconnection still takes the routing choice', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');
    setUseRemoteForChat(true);
    requestRemoteChat();

    // The other half of the SSE gap, and the likelier one: the disconnect is
    // lost and the connect survives. `remote_connected` is only ever emitted
    // by a fresh dial, so anything armed before it was armed for the machine
    // that dial is replacing — including a chat screen still on its way up.
    ingestRemoteEvent({ type: 'remote_connected', port: 41235 });
    expect(getRemoteState()).toMatchObject({ useForChat: false, chatRequestedAt: null });

    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(getRemoteState()).toMatchObject({ useForChat: false, chatModel: '' });
  });

  it('a peer swap takes the routing choice even when no model was ever named', () => {
    // `stillAimed` reads the peer from the *status*, not from the name's
    // attribution: the box can be ticked with the field still empty, and that
    // choice is no more transferable than a named one.
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);

    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(getRemoteState().useForChat).toBe(false);
  });

  it('the status read that replaces the placeholder keeps what was aimed at it', () => {
    // The mirror of the case above: placeholder to real peer is the same
    // connection learning its name, not a swap, so nothing is dropped.
    ingestRemoteEvent({ type: 'remote_connected', port: 41234 });
    setUseRemoteForChat(true);
    requestRemoteChat();

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(getRemoteState().useForChat).toBe(true);
    expect(getRemoteState().chatRequestedAt).not.toBeNull();
  });

  it('a choice made during the placeholder belongs to the machine being dialled', () => {
    // Why `stillAimed` reads the peer from the status and not from the name's
    // attribution, which is stickier. Here the two disagree: the name is still
    // attributed to the machine being left, while the box was ticked against
    // the dial that is replacing it. The tick wins — the panel was showing the
    // new connection when it was made.
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');

    ingestRemoteEvent({ type: 'remote_connected', port: 41235 });
    setUseRemoteForChat(true);
    requestRemoteChat();

    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(getRemoteState().useForChat).toBe(true);
    expect(getRemoteState().chatRequestedAt).not.toBeNull();
    // The name, typed for the machine being left, does not come along.
    expect(getRemoteState().chatModel).toBe('');
  });

  it('a status read that shows no connection also drops the preference', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);
    applyRemoteStatus(IDLE_STATUS);
    expect(getRemoteState().useForChat).toBe(false);
  });

  it('a machine that goes away keeps the connection, the choice and the model', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);
    setRemoteChatModel('qwen3');

    ingestRemoteEvent({ type: 'remote_away', port: 41234 });

    // The port is still bound and still dialling, so nothing about this is a
    // disconnection: the panel says away, and everything aimed at that
    // machine is waiting for it rather than being cleared.
    expect(getRemoteState().status?.connected?.away_for_s).toBe(0);
    expect(getRemoteState().status?.connected?.port).toBe(41234);
    expect(getRemoteState().useForChat).toBe(true);
    expect(getRemoteState().chatModel).toBe('qwen3');
  });

  it('a machine that answers again is here, with nothing to re-type', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);
    setRemoteChatModel('qwen3');
    ingestRemoteEvent({ type: 'remote_away', port: 41234 });

    ingestRemoteEvent({ type: 'remote_back', port: 41234 });

    expect(getRemoteState().status?.connected?.away_for_s).toBeNull();
    expect(getRemoteState().useForChat).toBe(true);
    expect(getRemoteState().chatModel).toBe('qwen3');
  });

  it('an away event with nothing connected changes nothing', () => {
    // The watcher is the only source of these and it only runs behind a
    // live connection, but a status read can race one in and leave the
    // store empty; a spread onto `null` would invent a connection.
    ingestRemoteEvent({ type: 'remote_away', port: 41234 });
    expect(getRemoteState().status).toBeNull();
  });
});
