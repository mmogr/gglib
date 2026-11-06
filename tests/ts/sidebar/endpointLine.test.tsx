/**
 * The endpoint line says only what the daemon said: its state word, the
 * address a client reaches it at, whether a request needs the key, and how
 * many models are loaded. Between an event and the read that follows it,
 * the address is withheld rather than guessed. ⧉ copies the whole address.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/components/ProxyDashboardModal', () => ({ ProxyDashboardModal: () => null }));
vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ revealSecret: async () => null, getSettings: async () => ({}) }),
}));

import { EndpointLine, secondLine, shownAddress } from '../../../src/sidebar/EndpointLine';
import { applyProxyStatus, ingestProxyEvent, resetProxyState } from '../../../src/services/proxyRegistry';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';
import { PlaceProviders } from '../places/harness';

const UP: ProxyStatus = {
  running: true,
  state: 'on',
  port: 8080,
  base_url: 'http://192.168.1.20:8080/v1',
  key: 'none_needed',
  pinned_model: null,
};

function serve(modelId: number, type: 'running' | 'stopped' = 'running') {
  act(() =>
    ingestServerEvent({ type, modelId: String(modelId), port: 9000 + modelId, updatedAt: Date.now(), modelName: `m${modelId}` }),
  );
}

const line = () => screen.getByRole('button', { name: /^endpoint,/i });

describe('the endpoint line', () => {
  beforeEach(() => resetProxyState());
  afterEach(() => {
    for (const id of [1, 2]) serve(id, 'stopped');
  });

  it('off: says Off, shows no address, and says what a click does', () => {
    applyProxyStatus({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null });
    render(<EndpointLine />, { wrapper: PlaceProviders });
    expect(line()).toHaveAccessibleName('Endpoint, Off');
    expect(screen.getByText('Off · click to turn on')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /copy the endpoint address/i })).not.toBeInTheDocument();
  });

  it('on: says On, the address without its scheme, and how many models are loaded', () => {
    applyProxyStatus(UP);
    serve(1);
    serve(2);
    render(<EndpointLine />, { wrapper: PlaceProviders });
    expect(line()).toHaveAccessibleName('Endpoint, On');
    expect(screen.getByText('192.168.1.20:8080/v1')).toBeInTheDocument();
    expect(screen.getByText('2 loaded')).toBeInTheDocument();

    serve(2, 'stopped');
    expect(screen.getByText('1 loaded')).toBeInTheDocument();
    serve(1, 'stopped');
    expect(screen.getByText('nothing loaded yet')).toBeInTheDocument();
  });

  it('pinned: says Pinned and names the one model it serves', () => {
    applyProxyStatus({ ...UP, state: 'pinned', pinned_model: 'qwen3-8b' });
    render(<EndpointLine />, { wrapper: PlaceProviders });
    expect(line()).toHaveAccessibleName('Endpoint, Pinned');
    expect(screen.getByText('only qwen3-8b')).toBeInTheDocument();
  });

  it('shows the key glyph only when a request must carry the key', () => {
    applyProxyStatus({ ...UP, key: 'required' });
    const { rerender } = render(<EndpointLine />, { wrapper: PlaceProviders });
    expect(screen.getByRole('img', { name: 'Key required' })).toBeInTheDocument();

    act(() => applyProxyStatus(UP));
    rerender(<EndpointLine />);
    expect(screen.queryByRole('img', { name: 'Key required' })).not.toBeInTheDocument();
  });

  it('between a start event and the daemon\'s answer, says On and withholds the old address', () => {
    applyProxyStatus({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null });
    render(<EndpointLine />, { wrapper: PlaceProviders });
    act(() => ingestProxyEvent({ type: 'proxy_started', port: 8080 }));
    expect(line()).toHaveAccessibleName('Endpoint, On');
    expect(screen.queryByRole('button', { name: /copy the endpoint address/i })).not.toBeInTheDocument();

    act(() => applyProxyStatus(UP));
    expect(screen.getByText('192.168.1.20:8080/v1')).toBeInTheDocument();
  });

  it('⧉ copies the whole address, scheme included, and says so', async () => {
    const user = userEvent.setup();
    applyProxyStatus(UP);
    render(<EndpointLine />, { wrapper: PlaceProviders });

    await user.click(screen.getByRole('button', { name: /copy the endpoint address/i }));

    expect(await navigator.clipboard.readText()).toBe('http://192.168.1.20:8080/v1');
    await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent('Endpoint address copied'));
    // The copy did not open the panel.
    expect(screen.queryByRole('heading', { name: 'OpenAI Proxy' })).not.toBeInTheDocument();
  });

  it('a click on the line opens today\'s proxy panel beside it, and a second closes it', async () => {
    applyProxyStatus(UP);
    render(<EndpointLine />, { wrapper: PlaceProviders });
    const user = userEvent.setup();
    await user.click(line());
    expect(screen.getByRole('heading', { name: 'OpenAI Proxy' })).toBeInTheDocument();
    await user.click(line());
    expect(screen.queryByRole('heading', { name: 'OpenAI Proxy' })).not.toBeInTheDocument();
  });

  it('a click anywhere else closes the panel; a click inside it does not', async () => {
    applyProxyStatus(UP);
    render(
      <>
        <EndpointLine />
        <p>elsewhere</p>
      </>,
      { wrapper: PlaceProviders },
    );
    const user = userEvent.setup();
    await user.click(line());
    await user.click(screen.getByRole('heading', { name: 'OpenAI Proxy' }));
    expect(screen.getByRole('heading', { name: 'OpenAI Proxy' })).toBeInTheDocument();
    await user.click(screen.getByText('elsewhere'));
    expect(screen.queryByRole('heading', { name: 'OpenAI Proxy' })).not.toBeInTheDocument();
  });
});

describe('the words of the endpoint line', () => {
  it('strips only the scheme from the address', () => {
    expect(shownAddress('http://127.0.0.1:8080/v1')).toBe('127.0.0.1:8080/v1');
    expect(shownAddress('http://[fe80::1]:8080/v1')).toBe('[fe80::1]:8080/v1');
  });

  it('second line: off, starting, the pin, none, a count', () => {
    const facts = { state: 'on' as const, baseUrl: 'http://127.0.0.1:8080/v1', key: null, pinnedModel: null };
    expect(secondLine({ ...facts, state: 'off' }, 3)).toBe('Off · click to turn on');
    expect(secondLine({ ...facts, state: 'starting' }, 3)).toBe('Starting');
    expect(secondLine({ ...facts, state: 'pinned', pinnedModel: 'qwen3-8b' }, 1)).toBe('only qwen3-8b');
    expect(secondLine(facts, 0)).toBe('nothing loaded yet');
    expect(secondLine(facts, 1)).toBe('1 loaded');
    expect(secondLine(facts, 2)).toBe('2 loaded');
  });
});
