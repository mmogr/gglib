/**
 * How a pairing code ended, said wherever the person is (`useOfferOutcomes`,
 * mounted at the app's root): a device paired by name, an expiry said and
 * the status read again so Invite comes back, a code that stopped working
 * early said once its grace has run, sharing stopping from elsewhere said,
 * a withdrawal (a Forget) and the app's own Stop sharing said by nobody,
 * and an ending from before the hook mounted not news.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

const refresh = vi.hoisted(() => vi.fn(async () => true));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: () => refresh() }));

import { useOfferOutcomes } from '../../../src/app/useOfferOutcomes';
import {
  applyRemoteStatus,
  clearOffer,
  ingestRemoteEvent,
  resetRemoteState,
  showOffer,
} from '../../../src/services/remoteRegistry';
import { PlaceProviders } from '../places/harness';
import { SHARED, invited, offerAnswer } from '../places/devicesFixtures';

function Outcomes() {
  useOfferOutcomes();
  return null;
}

const toasts = () => screen.getByTestId('toasts');
const mount = () => render(<Outcomes />, { wrapper: PlaceProviders });

describe('how a pairing code ended', () => {
  beforeEach(() => {
    resetRemoteState();
    applyRemoteStatus(SHARED);
    refresh.mockReset().mockResolvedValue(true);
  });
  afterEach(() => vi.useRealTimers());

  it('a device that paired is named', () => {
    mount();
    act(() => showOffer(offerAnswer('119284', { device: 'dev-99887766' })));
    act(() => ingestRemoteEvent({ type: 'remote_paired', peer: '0123456789ab' }));
    expect(toasts()).toHaveTextContent('Paired dev-99887766. It is listed under Devices.');
  });

  it('an expiry is said, and the status is read again', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mount();
    act(() => showOffer(offerAnswer('119284', { expires_in_s: 1 })));
    refresh.mockClear();
    await act(() => vi.advanceTimersByTimeAsync(1_500));
    expect(toasts()).toHaveTextContent('The pairing code expired and nobody paired.');
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('a code that stopped working early is said once its grace has run', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mount();
    act(() => showOffer(offerAnswer('119284', { device: 'dev-99887766' })));
    act(() => applyRemoteStatus({ ...SHARED, pairing_active: true, devices: [invited()] }));
    act(() => applyRemoteStatus({ ...SHARED, devices: [invited()] }));
    expect(toasts()).toHaveTextContent('');
    await act(() => vi.advanceTimersByTimeAsync(3_000));
    expect(toasts()).toHaveTextContent('stopped working before anyone paired');
  });

  it('sharing stopped from elsewhere is said, and nothing else', () => {
    mount();
    act(() => showOffer(offerAnswer('119284')));
    act(() => ingestRemoteEvent({ type: 'remote_disabled' }));
    expect(toasts()).toHaveTextContent('Sharing stopped, and the pairing code with it. Nobody paired.');
    expect(toasts()).not.toHaveTextContent(/expired|Paired/);
  });

  it('a withdrawal and the app’s own stop are said by nobody', () => {
    mount();
    act(() => showOffer(offerAnswer('119284', { device: 'dev-99887766' })));
    act(() => applyRemoteStatus({ ...SHARED, pairing_active: true, devices: [invited()] }));
    act(() => applyRemoteStatus(SHARED));
    act(() => showOffer(offerAnswer('222222')));
    act(() => clearOffer());
    act(() => ingestRemoteEvent({ type: 'remote_disabled' }));
    expect(toasts()).toHaveTextContent('');
  });

  it('an ending from before it mounted is not news', () => {
    showOffer(offerAnswer('119284', { device: 'dev-99887766' }));
    ingestRemoteEvent({ type: 'remote_paired', peer: '0123456789ab' });
    mount();
    expect(toasts()).toHaveTextContent('');
  });
});
