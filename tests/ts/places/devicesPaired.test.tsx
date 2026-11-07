/**
 * The Devices place's Paired devices, at `/devices`: each row's name and the
 * daemon's description of it, the id beside a label two rows share, no reach
 * dot; ⋯ Forget… asking first, Cancel retiring nothing, the forget awaited
 * and the list read again, `forgotten: false` and a failed read each said as
 * what they are; the empty line; the tool servers line only while shared,
 * Change › opening Settings › Tools; the fingerprint and peers line.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const transport = vi.hoisted(() => ({ forgetDevice: vi.fn() }));
const refresh = vi.hoisted(() => vi.fn(async () => true));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: () => refresh() }));

import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import type { RemoteDevice } from '../../../src/services/transport/types/remote';
import { renderApp } from './harness';
import { SHARED, device, invited } from './devicesFixtures';

const list = () => screen.getByRole('list');
const toasts = () => screen.getByTestId('toasts');

async function forgetVia(name: string) {
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: `More for ${name}` }));
  await user.click(screen.getByRole('menuitem', { name: 'Forget…' }));
  return user;
}

describe('Paired devices', () => {
  beforeEach(() => {
    resetRemoteState();
    transport.forgetDevice.mockReset();
    refresh.mockReset().mockResolvedValue(true);
  });

  it("names each row and prints the daemon's description, with no reach dot", () => {
    applyRemoteStatus({ ...SHARED, devices: [device(), { ...invited(), description: 'invited 1h ago, never joined · not admitted' }] });
    renderApp('/devices');
    expect(within(list()).getByText("Matt's phone")).toBeInTheDocument();
    expect(within(list()).getByText('last seen 1m ago')).toBeInTheDocument();
    // No label: the id is the name.
    expect(within(list()).getByText('dev-99887766')).toBeInTheDocument();
    expect(within(list()).getByText('invited 1h ago, never joined · not admitted')).toBeInTheDocument();
    // A label no other row shares stands alone.
    expect(within(list()).queryByText(/dev-0a1b2c3d/)).not.toBeInTheDocument();
    // The wire has no per-device reach.
    expect(list().querySelector('[data-kind]')).toBeNull();
  });

  it('two devices that named themselves the same thing each show their id', () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      devices: [device({ id: 'dev-aaaaaaaa', label: 'iPhone' }), device({ id: 'dev-bbbbbbbb', label: 'iPhone' })],
    });
    renderApp('/devices');
    expect(within(list()).getByText('iPhone (dev-aaaaaaaa)')).toBeInTheDocument();
    expect(within(list()).getByText('iPhone (dev-bbbbbbbb)')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'More for iPhone (dev-bbbbbbbb)' })).toBeInTheDocument();
  });

  it('a row from a daemon older than the description still names the device', () => {
    const older = { id: 'dev-33334444', label: 'iPad', joined_at: 1, redeemed_at: 1, last_seen: 1, admitted: true } as RemoteDevice;
    applyRemoteStatus({ ...SHARED, devices: [older] });
    renderApp('/devices');
    expect(within(list()).getByText('iPad')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'More for iPad' })).toBeInTheDocument();
  });

  it('says there are none yet, whether shared or not', () => {
    applyRemoteStatus(IDLE_STATUS);
    renderApp('/devices');
    expect(screen.getByText('No devices yet. Invite one above.')).toBeInTheDocument();
  });

  it('Forget… asks first with the id, and Cancel retires nothing', async () => {
    applyRemoteStatus({ ...SHARED, devices: [device()] });
    renderApp('/devices');
    const user = await forgetVia("Matt's phone");
    expect(await screen.findByText(`Forget "Matt's phone"?`)).toBeInTheDocument();
    expect(screen.getByText(/dev-0a1b2c3d: its key stops being admitted/)).toBeInTheDocument();
    expect(screen.getByText(/Every other device is untouched/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(transport.forgetDevice).not.toHaveBeenCalled();
    expect(within(list()).getByText("Matt's phone")).toBeInTheDocument();
  });

  it('a forget is awaited and the list read again before it says every other device is untouched', async () => {
    applyRemoteStatus({ ...SHARED, devices: [device()] });
    transport.forgetDevice.mockResolvedValue({ forgotten: true });
    renderApp('/devices');
    refresh.mockClear();
    const user = await forgetVia("Matt's phone");
    await user.click(await screen.findByRole('button', { name: 'Forget' }));
    expect(transport.forgetDevice).toHaveBeenCalledWith('dev-0a1b2c3d');
    expect(await within(toasts()).findByText("Forgot Matt's phone. Every other device is untouched.")).toBeInTheDocument();
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('a forget of something already gone says so rather than claiming success', async () => {
    applyRemoteStatus({ ...SHARED, devices: [device()] });
    transport.forgetDevice.mockResolvedValue({ forgotten: false });
    renderApp('/devices');
    const user = await forgetVia("Matt's phone");
    await user.click(await screen.findByRole('button', { name: 'Forget' }));
    expect(await within(toasts()).findByText('This machine held no device called dev-0a1b2c3d.')).toBeInTheDocument();
    expect(toasts()).not.toHaveTextContent(/Forgot/);
  });

  it('a forget whose read fails does not claim the list is current', async () => {
    applyRemoteStatus({ ...SHARED, devices: [device()] });
    transport.forgetDevice.mockResolvedValue({ forgotten: true });
    refresh.mockResolvedValue(false);
    renderApp('/devices');
    const user = await forgetVia("Matt's phone");
    await user.click(await screen.findByRole('button', { name: 'Forget' }));
    expect(await within(toasts()).findByText(/could not be read again/)).toBeInTheDocument();
    expect(toasts()).not.toHaveTextContent(/untouched/);
  });

  it('names the tool servers shared with them while shared, and Change › opens Settings › Tools', async () => {
    applyRemoteStatus({ ...SHARED, mcp_servers: ['tavily', 'github'] });
    const app = renderApp('/devices');
    expect(screen.getByText('Tool servers shared with them: tavily, github')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Change ›' }));
    expect(app.at()).toBe('/devices?settings=mcp');
  });

  it('hides the tool servers and the fingerprint while not shared', () => {
    applyRemoteStatus({ ...IDLE_STATUS, mcp_servers: ['tavily'], ticket_fingerprint: 'aa11bb22cc33' });
    renderApp('/devices');
    expect(screen.queryByText(/Tool servers shared with them/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Fingerprint/)).not.toBeInTheDocument();
  });

  it("says this Mac's fingerprint and its peers on one mono line", () => {
    applyRemoteStatus({
      ...SHARED,
      peers: [
        { fingerprint: '0123456789ab', path: 'direct' },
        { fingerprint: 'ba9876543210', path: 'relayed' },
      ],
    });
    renderApp('/devices');
    const line = screen.getByText('Fingerprint aa11bb22cc33 · 2 peers');
    expect(line).toHaveClass('font-mono', 'text-xs');
  });
});
