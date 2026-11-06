/**
 * The devices line says what the tunnel is doing in the daemon's words: the
 * machine this one has joined and its reach, sharing and how many paired,
 * a pairing code still waiting, or nothing. A click opens today's remote
 * panel beside it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: async () => true }));
vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ listMcpServers: async () => [], getSettings: async () => ({}) }),
}));

import { DevicesLine, devicesSaying } from '../../../src/sidebar/DevicesLine';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState, showOffer } from '../../../src/services/remoteRegistry';
import type { RemoteStatus } from '../../../src/services/transport/types/remote';
import { PlaceProviders } from '../places/harness';

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
    expect(devicesSaying(IDLE_STATUS, false)).toBeNull();
  });

  it('the joined machine and its reach, in the daemon\'s words', () => {
    expect(devicesSaying(joined('connected'), false)).toEqual({ mark: 'on', word: 'Connected', name: 'desk' });
    expect(devicesSaying(joined('away', 90), false)).toEqual({ mark: 'away', word: 'Away', name: 'desk' });
    expect(devicesSaying(joined('disconnected'), false)).toEqual({ mark: 'off', word: 'Disconnected', name: 'desk' });
  });

  it('sharing and how many devices are paired', () => {
    expect(devicesSaying({ ...IDLE_STATUS, enabled: true, devices: [] }, false)).toEqual({ mark: 'on', word: 'Sharing · 0 paired' });
    expect(devicesSaying({ ...IDLE_STATUS, enabled: true, devices: [device, { ...device, id: 'dev-2' }] }, false)).toEqual({
      mark: 'on',
      word: 'Sharing · 2 paired',
    });
  });

  it('a code waiting, over everything else', () => {
    expect(devicesSaying(joined('connected'), true)).toEqual({ mark: 'attention', word: 'Code waiting' });
  });

  it('a turning mark before the daemon has answered', () => {
    expect(devicesSaying(null, false)).toEqual({ mark: 'changing', word: '' });
  });
});

describe('the devices line', () => {
  beforeEach(() => {
    resetRemoteState();
  });

  it('is named by what it says, and says nothing when there is nothing', () => {
    applyRemoteStatus(IDLE_STATUS);
    render(<DevicesLine />, { wrapper: PlaceProviders });
    expect(line()).toHaveAccessibleName('Devices');
  });

  it('names the joined machine and its reach', () => {
    applyRemoteStatus(joined('away', 120));
    render(<DevicesLine />, { wrapper: PlaceProviders });
    expect(line()).toHaveAccessibleName('Devices, desk Away');
    expect(screen.getByText('desk')).toBeInTheDocument();
    expect(screen.getByText('Away')).toBeInTheDocument();
  });

  it('says a code is waiting on the line itself, so a closed panel is not a lost code', () => {
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true });
    showOffer({
      ticket: 't',
      code: '119284',
      pairing: 't-119284',
      expires_in_s: 120,
      device: 'dev-99887766',
      mcp_servers: [],
      already_up: false,
    });
    render(<DevicesLine />, { wrapper: PlaceProviders });
    expect(line()).toHaveAccessibleName('Devices, Code waiting');
    expect(line()).toHaveAttribute('title', 'A pairing code is waiting');
  });

  it('a click opens today\'s remote panel, and a second closes it', async () => {
    applyRemoteStatus(IDLE_STATUS);
    render(<DevicesLine />, { wrapper: PlaceProviders });
    const user = userEvent.setup();
    await user.click(line());
    expect(screen.getByRole('heading', { name: 'Remote' })).toBeInTheDocument();
    await user.click(line());
    expect(screen.queryByRole('heading', { name: 'Remote' })).not.toBeInTheDocument();
  });
});
