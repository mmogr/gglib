/**
 * The Devices place's This Mac block and pairing sheet, at `/devices`
 * through the real routes: the Off sentence and Share with my devices; the
 * key confirm only when the endpoint does not already require a key, Cancel
 * doing nothing, and Turn on enabling with `invite` only while no device has
 * joined (DV-2); the sheet opening from the offer with the code in 3+3, the
 * QR from the uppercased string, and the time left from the code's own
 * deadline, kept across leaving the page (DV-1); "<label> joined." then
 * closing; "Expired." with New code; Invite dead while a code is live; Stop
 * sharing dropping the code before it asks; the fifteen-second read while a
 * terminal's code is live.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { renderToStaticMarkup } from 'react-dom/server';
import { QRCodeSVG } from 'qrcode.react';

const transport = vi.hoisted(() => ({
  enableRemote: vi.fn(),
  inviteRemote: vi.fn(),
  disableRemote: vi.fn(),
  forgetDevice: vi.fn(),
}));
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
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { IDLE_STATUS, applyRemoteStatus, getRemoteState, ingestRemoteEvent, resetRemoteState } from '../../../src/services/remoteRegistry';
import { applyProxyStatus, resetProxyState } from '../../../src/services/proxyRegistry';
import { renderApp } from './harness';
import { SHARED, TICKET, device, invited, offerAnswer } from './devicesFixtures';

const share = () => screen.getByRole('button', { name: 'Share with my devices' });
const invite = () => screen.getByRole('button', { name: 'Invite a device' });
const sheet = () => screen.getByRole('region', { name: 'Pairing code' });

function keyRequired() {
  applyProxyStatus({ running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'required', pinned_model: null, pinned_to: null });
}

describe('This Mac', () => {
  beforeEach(() => {
    resetRemoteState();
    resetProxyState();
    applyRemoteStatus(IDLE_STATUS);
    Object.values(transport).forEach((fn) => fn.mockReset());
    refresh.mockReset().mockResolvedValue(true);
  });
  afterEach(() => vi.useRealTimers());

  it('says what sharing is for while off, under the header', () => {
    renderApp('/devices');
    expect(screen.getByRole('heading', { name: 'Devices' })).toBeInTheDocument();
    expect(screen.getByText('End-to-end encrypted. No account.')).toBeInTheDocument();
    expect(screen.getByText("Let your laptop and phone use this Mac's models.")).toBeInTheDocument();
    expect(share()).toBeEnabled();
    // Opening the place asks the daemon again.
    expect(refresh).toHaveBeenCalled();
  });

  it('asks about the key first when the endpoint does not require one, and Cancel does nothing', async () => {
    renderApp('/devices');
    const user = userEvent.setup();
    await user.click(share());
    expect(await screen.findByText('Clients on this Mac will need an API key from now on.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(transport.enableRemote).not.toHaveBeenCalled();
  });

  it('Turn on enables with a code while no device has joined, and the sheet opens on it', async () => {
    transport.enableRemote.mockResolvedValue(offerAnswer('483920', { device: 'dev-5e6f7a8b' }));
    renderApp('/devices');
    const user = userEvent.setup();
    await user.click(share());
    await user.click(await screen.findByRole('button', { name: 'Turn on' }));

    expect(transport.enableRemote).toHaveBeenCalledWith({ invite: true });
    const code = await screen.findByText('483 920');
    expect(within(sheet()).getByText(`gglib remote join ${TICKET}-483920`)).toBeInTheDocument();
    expect(within(sheet()).getByText('This code pairs dev-5e6f7a8b.')).toBeInTheDocument();
    expect(within(sheet()).getByText('2:00')).toBeInTheDocument();
    expect(code).toBeInTheDocument();
  });

  it('asks nothing when the endpoint already requires the key, and mints no code once a device has joined', async () => {
    keyRequired();
    applyRemoteStatus({ ...IDLE_STATUS, devices: [device()] });
    transport.enableRemote.mockResolvedValue({ ticket: TICKET, mcp_servers: [], already_up: false });
    renderApp('/devices');
    await userEvent.setup().click(share());
    expect(transport.enableRemote).toHaveBeenCalledWith({ invite: false });
    expect(screen.queryByText(/will need an API key/)).not.toBeInTheDocument();
    // An answer with no code opens no sheet.
    expect(screen.queryByRole('region', { name: 'Pairing code' })).not.toBeInTheDocument();
  });

  it("asks for a code when the only row is an invite nobody took, or the daemon sends no roster", async () => {
    keyRequired();
    transport.enableRemote.mockResolvedValue({ ticket: TICKET, mcp_servers: [], already_up: false });
    applyRemoteStatus({ ...IDLE_STATUS, devices: [invited()] });
    const first = renderApp('/devices');
    await userEvent.setup().click(share());
    expect(transport.enableRemote).toHaveBeenLastCalledWith({ invite: true });
    first.unmount();

    const older: Record<string, unknown> = { ...IDLE_STATUS };
    delete older.devices;
    applyRemoteStatus(older as unknown as typeof IDLE_STATUS);
    renderApp('/devices');
    await userEvent.setup().click(share());
    expect(transport.enableRemote).toHaveBeenLastCalledWith({ invite: true });
  });

  it('shows "Finding a relay…" in place while enabling, and a refusal in the daemon\'s words', async () => {
    keyRequired();
    let fail: (e: Error) => void = () => {};
    transport.enableRemote.mockImplementation(() => new Promise((_, reject) => (fail = reject)));
    renderApp('/devices');
    await userEvent.setup().click(share());
    expect(screen.getByText('Finding a relay…')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Share with my devices' })).not.toBeInTheDocument();
    await act(async () => fail(new Error('no relay answered within 10s')));
    expect(await screen.findByText('no relay answered within 10s')).toBeInTheDocument();
  });

  it('says a switch on with nothing bound, and Share is still there to press', () => {
    applyRemoteStatus({ ...IDLE_STATUS, remote_enabled: true, enabled: false });
    renderApp('/devices');
    expect(screen.getByText(/nothing is bound yet/)).toBeInTheDocument();
    expect(share()).toBeEnabled();
  });

  it('Invite offers one more code with no arguments, and is dead while it is live', async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('119284'));
    renderApp('/devices');
    expect(screen.getByText('Shared with your devices')).toBeInTheDocument();
    await userEvent.setup().click(invite());
    expect(transport.inviteRemote).toHaveBeenCalledWith();
    expect(await screen.findByText('119 284')).toBeInTheDocument();
    expect(invite()).toBeDisabled();
  });

  it('Invite is dead while the status says a code is live, though none is on offer here', () => {
    applyRemoteStatus({ ...SHARED, pairing_active: true });
    renderApp('/devices');
    expect(invite()).toBeDisabled();
  });

  it('a refused invite re-reads the status and says why', async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockRejectedValue(new Error('a pairing code is already live'));
    renderApp('/devices');
    refresh.mockClear();
    await userEvent.setup().click(invite());
    expect(await screen.findByText('a pairing code is already live')).toBeInTheDocument();
    expect(refresh).toHaveBeenCalled();
  });

  it('the QR is the pairing string uppercased, at level M with a quiet zone, black on white', async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('483920'));
    renderApp('/devices');
    await userEvent.setup().click(invite());
    const qr = await screen.findByRole('img', { name: 'Pairing QR code' });
    const expected = document.createElement('div');
    expected.innerHTML = renderToStaticMarkup(<QRCodeSVG value={`${TICKET.toUpperCase()}-483920`} level="M" marginSize={4} />);
    const modules = (svg: Element) => svg.querySelectorAll('path')[1]?.getAttribute('d');
    expect(modules(qr)).toBeTruthy();
    expect(modules(qr)).toBe(modules(expected.querySelector('svg')!));
    const [background, foreground] = qr.querySelectorAll('path');
    expect(background.getAttribute('fill')).toBe('#ffffff');
    expect(foreground.getAttribute('fill')).toBe('#000000');
    expect(screen.getByText('Scan with ggchat')).toBeInTheDocument();
  });

  it("counts down to the code's own deadline, and the code is back after leaving the page", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('119284'));
    const app = renderApp('/devices');
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(invite());
    await screen.findByText('119 284');

    await act(() => app.router.navigate('/library'));
    expect(screen.queryByText('119 284')).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(30_000));
    await act(() => app.router.navigate('/devices'));
    expect(screen.getByText('119 284')).toBeInTheDocument();
    expect(within(sheet()).getByText(/^1:(29|30)$/)).toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(5_000));
    expect(within(sheet()).getByText(/^1:(24|25)$/)).toBeInTheDocument();
  });

  it('says the device joined by its label, then closes', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('119284', { device: 'dev-99887766' }));
    renderApp('/devices');
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(invite());
    await screen.findByText('119 284');

    act(() => applyRemoteStatus({ ...SHARED, devices: [device({ id: 'dev-99887766', label: 'Laptop' })] }));
    expect(screen.getByText('Laptop joined.')).toBeInTheDocument();
    expect(screen.queryByText('119 284')).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(3_000));
    expect(screen.queryByText('Laptop joined.')).not.toBeInTheDocument();
  });

  it('says Expired. with New code, which offers a fresh one', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValueOnce(offerAnswer('111111', { expires_in_s: 1 })).mockResolvedValueOnce(offerAnswer('222222'));
    renderApp('/devices');
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    await user.click(invite());
    await screen.findByText('111 111');
    await act(() => vi.advanceTimersByTimeAsync(1_500));
    expect(screen.getByText('Expired.')).toBeInTheDocument();
    expect(screen.queryByText('111 111')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'New code' }));
    expect(await screen.findByText('222 222')).toBeInTheDocument();
    expect(screen.queryByText('Expired.')).not.toBeInTheDocument();
  });

  it('forgetting the invited device takes its code off and gives Invite back', async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('119284', { device: 'dev-99887766' }));
    transport.forgetDevice.mockResolvedValue({ forgotten: true });
    refresh
      .mockImplementationOnce(async () => true) // the place opening
      .mockImplementationOnce(async () => {
        applyRemoteStatus({ ...SHARED, pairing_active: true, devices: [invited()] });
        return true;
      })
      .mockImplementationOnce(async () => {
        applyRemoteStatus(SHARED);
        return true;
      });
    renderApp('/devices');
    const user = userEvent.setup();
    await user.click(invite());
    await screen.findByText('119 284');
    await user.click(await screen.findByRole('button', { name: 'More for dev-99887766' }));
    await user.click(screen.getByRole('menuitem', { name: 'Forget…' }));
    await user.click(await screen.findByRole('button', { name: 'Forget' }));

    await waitFor(() => expect(screen.queryByText('119 284')).not.toBeInTheDocument());
    expect(invite()).toBeEnabled();
  });

  it('Stop sharing drops the code before it asks, whenever its event lands', async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('119284'));
    let offerAtRequest: unknown = 'unasked';
    transport.disableRemote.mockImplementation(async () => {
      offerAtRequest = getRemoteState().offer;
      ingestRemoteEvent({ type: 'remote_disabled' });
      return IDLE_STATUS;
    });
    renderApp('/devices');
    const user = userEvent.setup();
    await user.click(invite());
    await screen.findByText('119 284');
    await user.click(screen.getByRole('button', { name: 'Stop sharing' }));

    expect(offerAtRequest).toBeNull();
    // The app's own stop is no ending to announce.
    expect(getRemoteState().ended).toBeNull();
    expect(await screen.findByRole('button', { name: 'Share with my devices' })).toBeInTheDocument();
  });

  it("reads the status every fifteen seconds while a terminal's code is live, until Invite comes back", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    applyRemoteStatus({ ...SHARED, pairing_active: true });
    renderApp('/devices');
    expect(invite()).toBeDisabled();
    refresh.mockImplementation(async () => {
      applyRemoteStatus(SHARED);
      return true;
    });
    await act(() => vi.advanceTimersByTimeAsync(15_000));
    expect(invite()).toBeEnabled();
  });

  it('an ending from while the page was closed is not said when it opens again', async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('119284', { device: 'dev-99887766' }));
    const app = renderApp('/devices');
    await userEvent.setup().click(invite());
    await screen.findByText('119 284');
    await act(() => app.router.navigate('/library'));
    act(() => applyRemoteStatus({ ...SHARED, devices: [device({ id: 'dev-99887766', label: 'Laptop' })] }));
    await act(() => app.router.navigate('/devices'));
    expect(screen.getByRole('heading', { name: 'Devices' })).toBeInTheDocument();
    expect(screen.queryByText('Laptop joined.')).not.toBeInTheDocument();
  });

  it('"Expired." is offered only while the tunnel is up', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('111111', { expires_in_s: 1 }));
    renderApp('/devices');
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(invite());
    await screen.findByText('111 111');
    await act(() => vi.advanceTimersByTimeAsync(1_500));
    expect(screen.getByText('Expired.')).toBeInTheDocument();
    act(() => applyRemoteStatus(IDLE_STATUS));
    expect(screen.queryByText('Expired.')).not.toBeInTheDocument();
  });

  it("the join line's ⧉ copies the pairing string, which the Join field takes", async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockResolvedValue(offerAnswer('119284'));
    renderApp('/devices');
    const user = userEvent.setup();
    await user.click(invite());
    await user.click(await screen.findByRole('button', { name: 'Copy the pairing string' }));
    expect(await navigator.clipboard.readText()).toBe(`${TICKET}-119284`);
    expect(await screen.findByText('Copied')).toBeInTheDocument();
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();
  });
});
