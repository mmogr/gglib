/**
 * The "Reset to default" link beside Models Directory.
 *
 * It sits in that field's `action` slot, so it must reset that field and
 * nothing else. It used to reset thirteen further settings — the title
 * prompt, fit indicators, client-sampling trust, proxy loop detection, the
 * download path, bind host, LAN sharing, three desktop toggles and three
 * agent guards — and `handleSubmit` sent every one of them on the next save.
 *
 * That never fired in production: the gate read `info?.defaultPath`, a key
 * the wire has never carried (`ModelsDirectoryInfo` is `default_path`, and
 * Rust derives no `rename_all`), so the button did not render at all. Fixing
 * the spelling is what would have made the wide reset reachable, which is why
 * this test exists at the moment the spelling was fixed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsModal } from '../../../src/components/SettingsModal';

const info = {
  path: '/custom/models',
  default_path: '/home/u/.local/share/llama_models',
  source: 'explicit',
};

const settings = {
  proxyPort: 8081,
  llamaBasePort: 9100,
  maxDownloadQueueSize: 5,
  defaultContextSize: 8192,
  proxyApiKey: 'sk-kept',
  bindHost: '0.0.0.0',
  shareLan: true,
  proxyAutostart: true,
  closeToTray: true,
  startAtLogin: true,
  trustClientSampling: true,
  proxyLoopDetection: false,
  showMemoryFitIndicators: false,
  titleGenerationPrompt: 'a prompt the user wrote',
  agenticSampling: false,
  toolCallRepair: false,
  maxStagnationSteps: 7,
  defaultDownloadPath: '/downloads',
  inferenceProfiles: [],
};

vi.mock('../../../src/hooks/useModelsDirectory', () => ({
  useModelsDirectory: () => ({
    info,
    loading: false,
    saving: false,
    error: null,
    refresh: vi.fn(),
    save: vi.fn(),
  }),
}));

vi.mock('../../../src/hooks/useSettings', () => ({
  useSettings: () => ({
    settings,
    loading: false,
    error: null,
    refresh: vi.fn(),
    save: vi.fn(),
  }),
}));

vi.mock('../../../src/hooks/useMcpServers', () => ({
  useMcpServers: () => ({
    servers: [],
    tools: [],
    loading: false,
    error: null,
    refresh: vi.fn(),
    callTool: vi.fn(),
  }),
}));

vi.mock('../../../src/hooks/useModels', () => ({
  useModels: () => ({ models: [], loading: false, error: null, refresh: vi.fn() }),
}));

describe('SettingsModal — reset to default', () => {
  beforeEach(() => vi.clearAllMocks());

  it('resets the models directory and leaves every other field alone', async () => {
    const user = userEvent.setup();
    render(<SettingsModal isOpen onClose={vi.fn()} />);

    const modelsDir = document.querySelector<HTMLInputElement>('#models-dir-input');
    const bindHost = document.querySelector<HTMLInputElement>('#bind-host-input');
    const shareLan = document.querySelector<HTMLInputElement>('#share-lan-input');

    expect(modelsDir?.value).toBe('/custom/models');
    expect(bindHost?.value).toBe('0.0.0.0');
    expect(shareLan?.checked).toBe(true);

    await user.click(screen.getByRole('button', { name: /reset to default/i }));

    // The field the link belongs to.
    expect(modelsDir?.value).toBe('/home/u/.local/share/llama_models');

    // The network pair, untouched. These two are the assertion that matters:
    // the old handler called `network.reset()`, whose DEFAULTS are
    // `{ bindHost: '', shareLan: false }` — a true reset, not a revert to
    // saved. The port and API-key fields it also touched were reset to their
    // *saved* values, so they would look unchanged here and prove nothing.
    // The same is true of the three desktop toggles and the four settings
    // behind the collapsed Advanced disclosure.
    expect(bindHost?.value).toBe('0.0.0.0');
    expect(shareLan?.checked).toBe(true);
  });
});
