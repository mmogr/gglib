/**
 * The app shell and the llama.cpp install prompt.
 *
 * The shell opens the prompt when the daemon says llama.cpp is missing, which
 * a browser tab is now told as the desktop app is. The prompt runs the install
 * itself, so the shell's part is what follows one: read the status again, and
 * close the prompt and bring the menu up to date once its last line has been
 * on screen for two seconds. The prompt is a stand-in that shows what it was
 * given; `LlamaInstallModal.test.tsx` covers the real one.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';

interface PromptProps {
  canDownload?: boolean;
  error?: string | null;
  onSkip?: () => void;
  onInstalled?: () => void;
}

vi.mock('../../../src/pages/ModelControlCenterPage', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/components/Header', () => ({ default: () => null }));
vi.mock('../../../src/components/SettingsModal', () => ({ default: () => null }));
vi.mock('../../../src/components/SetupWizard', () => ({ default: () => null }));
vi.mock('../../../src/components/LlamaInstallModal', () => ({
  default: ({ canDownload, error, onSkip, onInstalled }: PromptProps) => (
    <div role="dialog" aria-label="Install llama.cpp" data-can-download={String(canDownload)}>
      {error}
      <button onClick={onInstalled}>finish the install</button>
      <button onClick={onSkip}>skip</button>
    </div>
  ),
}));
vi.mock('../../../src/hooks/useServers', () => {
  const servers = { servers: [], stopServer: vi.fn() };
  return { useServers: () => servers };
});
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
  syncMenuStateSilent: vi.fn(),
  listenToMenuEvents: vi.fn(async () => () => {}),
  MENU_EVENTS: {},
}));
vi.mock('../../../src/services/serverEvents', () => ({ initServerEvents: vi.fn(), cleanupServerEvents: vi.fn() }));
vi.mock('../../../src/services/proxyEvents', () => ({ initProxyEvents: vi.fn(), cleanupProxyEvents: vi.fn() }));
vi.mock('../../../src/services/remoteEvents', () => ({ initRemoteEvents: vi.fn(), cleanupRemoteEvents: vi.fn() }));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => ({ getSettings: async () => ({}) }) }));
vi.mock('../../../src/services/transport/api/setup', () => ({ getSetupStatus: vi.fn() }));
vi.mock('../../../src/services/tools', () => ({ syncBuiltinTools: async () => {} }));

import App from '../../../src/App';
import { syncMenuStateSilent } from '../../../src/services/platform';
import { getSetupStatus } from '../../../src/services/transport/api/setup';
import type { SetupStatus } from '../../../src/types/setup';

const getStatus = vi.mocked(getSetupStatus);
const status = (llamaInstalled: boolean, llamaCanDownload: boolean) =>
  ({ setupCompleted: true, llamaInstalled, llamaCanDownload }) as SetupStatus;

/** Let the status read land and the shell act on it: a full turn of the event loop. */
const settled = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

/** Render the shell and wait until it has asked for the llama.cpp status. */
async function renderShell() {
  render(<App />);
  await screen.findByRole('main', { name: 'Library' });
  // The first read decides whether setup is done; the second is the shell's.
  await vi.waitFor(() => expect(getStatus).toHaveBeenCalledTimes(2));
}

describe('the app shell\'s llama.cpp install prompt', () => {
  beforeEach(() => getStatus.mockReset());
  afterEach(() => vi.useRealTimers());

  it('opens when the daemon reports no llama.cpp, saying whether a prebuilt binary can be downloaded', async () => {
    getStatus.mockResolvedValue(status(false, true));
    await renderShell();

    expect(await screen.findByRole('dialog')).toHaveAttribute('data-can-download', 'true');
  });

  it('stays closed when llama.cpp is installed', async () => {
    getStatus.mockResolvedValue(status(true, true));
    await renderShell();
    await settled();

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('stays closed when the status cannot be read', async () => {
    getStatus.mockResolvedValueOnce(status(false, true)).mockRejectedValueOnce(new Error('offline'));
    await renderShell();
    await settled();

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('says so when no prebuilt binary can be downloaded, and closes when skipped', async () => {
    getStatus.mockResolvedValue(status(false, false));
    await renderShell();
    expect(await screen.findByRole('dialog')).toHaveAttribute('data-can-download', 'false');

    fireEvent.click(screen.getByRole('button', { name: 'skip' }));

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('after an install, reads the status again at once, and two seconds later closes and syncs the menu', async () => {
    getStatus.mockResolvedValue(status(false, true));
    await renderShell();
    const finish = await screen.findByRole('button', { name: 'finish the install' });
    getStatus.mockResolvedValue(status(true, true));
    vi.useFakeTimers();

    fireEvent.click(finish);
    expect(getStatus).toHaveBeenCalledTimes(3);

    await act(() => vi.advanceTimersByTimeAsync(1999));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(syncMenuStateSilent).not.toHaveBeenCalled();

    await act(() => vi.advanceTimersByTimeAsync(1));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(syncMenuStateSilent).toHaveBeenCalledTimes(1);
  });
});
