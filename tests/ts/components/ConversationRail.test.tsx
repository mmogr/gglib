/**
 * The chat rail's foot names the paired machine as every surface does: by
 * the name its stored pairing holds, in words when it gave none, and never
 * by its fingerprint, which is identity and is not shown. Joined, the foot
 * is the switch to that machine's chats; not joined, a chat with it names
 * it beside "not connected".
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { ConversationRail } from '../../../src/components/ConversationListPanel/ConversationRail';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  ingestRemoteEvent,
  resetRemoteState,
} from '../../../src/services/remoteRegistry';
import type { ChatSource, RemoteConnection } from '../../../src/services/transport';

/** The paired machine's fingerprint: its identity, and never on screen. */
const FINGERPRINT = '3ca82708b995';

const CONNECTED: RemoteConnection = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: FINGERPRINT,
  path: 'direct',
  away_for_s: null,
};

function rail(remote: boolean, source: ChatSource = 'this') {
  render(
    <ConversationRail
      onNewConversation={() => {}}
      onSearch={() => {}}
      listOpen
      canFold={false}
      onToggleList={() => {}}
      listId="conversations"
      remote={remote}
      running={0}
      unread={0}
      source={source}
      onSource={() => {}}
    />,
  );
}

beforeEach(() => resetRemoteState());
afterEach(() => resetRemoteState());

describe('ConversationRail, the paired machine', () => {
  it("joined, the switch offers that machine's chats by its name", () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      stored_ticket_fingerprint: FINGERPRINT,
      paired_name: 'desk',
      has_remote_key: true,
      connected: CONNECTED,
    });
    rail(false);

    const far = screen.getByRole('button', { name: /desk's chats/ });
    expect(far).toHaveAttribute('title', 'The chats kept on desk, read through the tunnel');
    expect(far).toHaveTextContent('direct');
    expect(document.body.innerHTML).not.toContain(FINGERPRINT);
  });

  it('a machine that gave no name is offered in words, not by its fingerprint', () => {
    applyRemoteStatus({ ...IDLE_STATUS, stored_ticket_fingerprint: FINGERPRINT, connected: CONNECTED });
    rail(false);

    expect(screen.getByRole('button', { name: /the paired machine's chats/ })).toBeInTheDocument();
    expect(document.body.innerHTML).not.toContain(FINGERPRINT);
  });

  it('a join to another machine does not name the one it replaced', () => {
    // Between the join event and the status read, the name held is the last
    // read's: on a join with a code to a new machine, the old machine's.
    applyRemoteStatus({
      ...IDLE_STATUS,
      stored_ticket_fingerprint: FINGERPRINT,
      paired_name: 'laptop',
      has_remote_key: true,
      connected: CONNECTED,
    });
    ingestRemoteEvent({ type: 'remote_joined', port: 41235 });
    rail(false);

    expect(screen.getByRole('button', { name: /the paired machine's chats/ })).toBeInTheDocument();
    expect(document.body.innerHTML).not.toContain('laptop');
  });

  it('a long name with nowhere to break still wraps inside the rail', () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      stored_ticket_fingerprint: FINGERPRINT,
      paired_name: 'workstation_main',
      connected: CONNECTED,
    });
    rail(false);

    expect(screen.getByText("workstation_main's chats")).toHaveClass('wrap-anywhere');
  });

  it('a chat with it while it is not connected names it, and says so', () => {
    applyRemoteStatus({ ...IDLE_STATUS, stored_ticket_fingerprint: FINGERPRINT, paired_name: 'desk' });
    rail(true);

    expect(screen.getByText('desk')).toBeInTheDocument();
    expect(screen.getByText('not connected')).toBeInTheDocument();
    expect(screen.getByTitle('desk is not connected')).toBeInTheDocument();
    expect(document.body.innerHTML).not.toContain(FINGERPRINT);
  });
});
