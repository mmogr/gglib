/**
 * The devices line says what the tunnel is doing in the daemon's words: the
 * machine this one has joined and its reach, sharing and how many paired,
 * a pairing code still on offer as "code m:ss" counting down to its own
 * deadline, or nothing. A click opens the Devices place.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  listModels: vi.fn(async () => []),
  getSettings: vi.fn(async () => ({})),
  getProxyStatus: vi.fn(async () => ({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null, pinned_to: null })),
  revealSecret: vi.fn(async () => null),
}));

vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: async () => true }));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { devicesSaying } from '../../../src/sidebar/DevicesLine';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState, showOffer } from '../../../src/services/remoteRegistry';
import type { RemoteStatus } from '../../../src/services/transport/types/remote';
import { renderShell } from '../places/harness';

/** Joined to `desk`, reached as `reach` says. */
function joined(reach: RemoteStatus['reach'], awayFor: number | null = null): RemoteStatus {
  return {
    ...IDLE_STATUS,
    reach,
    paired_name: 'desk',
    connected: {
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
      ticket_fingerprint: '3ca82708b995',
      path: 'direct',
      away_for_s: awayFor,
    },
  };
}

const device = {
  id: 'dev-0a1b2c3d',
  joined_at: 1,
  redeemed_at: 1,
  last_seen: 1,
  peer: null,
  admitted: true,
  recorded: true,
  description: 'last seen 1m ago',
  joined: true,
};

const line = () => screen.getByRole('button', { name: /^devices/i });

describe('what the devices line says', () => {
  it('nothing while the tunnel is off and nothing is joined', () => {
    expect(devicesSaying(IDLE_STATUS, null)).toBeNull();
  });

  it('the joined machine and its reach, in the daemon\'s words', () => {
    expect(devicesSaying(joined('connected'), null)).toEqual({ mark: 'on', word: 'Connected', name: 'desk' });
    expect(devicesSaying(joined('away', 90), null)).toEqual({ mark: 'away', word: 'Away', name: 'desk' });
    expect(devicesSaying(joined('disconnected'), null)).toEqual({ mark: 'off', word: 'Disconnected', name: 'desk' });
  });

  it('sharing and how many devices are paired', () => {
    expect(devicesSaying({ ...IDLE_STATUS, enabled: true, devices: [] }, null)).toEqual({ mark: 'on', word: 'Sharing · 0 paired' });
    expect(devicesSaying({ ...IDLE_STATUS, enabled: true, devices: [device, { ...device, id: 'dev-2' }] }, null)).toEqual({
      mark: 'on',
      word: 'Sharing · 2 paired',
    });
  });

  it('sharing with a daemon that sends no roster counts none', () => {
    const older: Record<string, unknown> = { ...IDLE_STATUS, enabled: true };
    delete older.devices;
    expect(devicesSaying(older as unknown as RemoteStatus, null)).toEqual({ mark: 'on', word: 'Sharing · 0 paired' });
  });

  it('a code on offer as its time left, over everything else, in plain text: ▲ is for warnings', () => {
    expect(devicesSaying(joined('connected'), 102)).toEqual({ mark: null, word: 'code 1:42' });
    expect(devicesSaying(null, 9)).toEqual({ mark: null, word: 'code 0:09' });
  });

  it('a turning mark before the daemon has answered', () => {
    expect(devicesSaying(null, null)).toEqual({ mark: 'changing', word: '' });
  });
});

describe('the devices line', () => {
  beforeEach(() => {
    resetRemoteState();
  });
  afterEach(() => vi.useRealTimers());

  it('is named by what it says, and says nothing when there is nothing', () => {
    applyRemoteStatus(IDLE_STATUS);
    renderShell('/library');
    expect(line()).toHaveAccessibleName('Devices');
  });

  it('names the joined machine and its reach', () => {
    applyRemoteStatus(joined('away', 120));
    renderShell('/library');
    expect(line()).toHaveAccessibleName('Devices, desk Away');
    expect(screen.getByText('desk')).toBeInTheDocument();
    expect(screen.getByText('Away')).toBeInTheDocument();
  });

  it("says a code on offer as its time left, counting down to the code's deadline", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true });
    showOffer({
      ticket: 't',
      code: '119284',
      pairing: 't-119284',
      expires_in_s: 102,
      device: 'dev-99887766',
      mcp_servers: [],
      already_up: false,
    });
    renderShell('/library');
    expect(line()).toHaveAccessibleName('Devices, code 1:42');
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(line()).toHaveAccessibleName(/^Devices, code 1:3[12]$/);
    expect(screen.getByText(/^code 1:3[12]$/)).toBeInTheDocument();
    // No shape beside it: a code on offer is not a warning.
    expect(line().querySelector('[data-kind]')).toBeNull();
  });

  it('a click opens the Devices place, and the line is then the open row', async () => {
    applyRemoteStatus(IDLE_STATUS);
    const app = renderShell('/library');
    expect(line()).not.toHaveAttribute('aria-current');
    await userEvent.setup().click(line());
    expect(app.at()).toBe('/devices');
    expect(await screen.findByRole('heading', { name: 'Devices' })).toBeInTheDocument();
    expect(line()).toHaveAttribute('aria-current', 'page');
  });
});
