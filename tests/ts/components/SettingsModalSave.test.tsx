/**
 * What a Save in the settings dialog sends (#1059).
 *
 * The dialog reads the settings once, when it opens. A Save used to send
 * every field it showed, so it put each one back to what it had read and
 * undid anything written meanwhile — a proxy key `gglib remote enable` stored
 * while the dialog was open, or a port set from a terminal. A Save now sends
 * the fields the person changed and nothing else.
 *
 * Each test asserts the whole request with `toEqual`, so an unchanged field
 * from any group of the form — the General tab's own inputs, the network
 * pair, the agent guards or the desktop toggles — fails it by being there.
 * The table changes each field the dialog sends in turn, so a field that is
 * dropped from its group's request, or sent under another's name, fails too.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsModal } from '../../../src/components/SettingsModal';
import type { UpdateSettingsRequest } from '../../../src/types';

const { save } = vi.hoisted(() => ({ save: vi.fn() }));

const info = {
  path: '/custom/models',
  default_path: '/home/u/.local/share/llama_models',
  source: 'explicit',
};

/** Every field the dialog shows holds a value, so any of them could leak. */
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
  loopGuardMode: 'refuse',
  showMemoryFitIndicators: false,
  titleGenerationPrompt: 'a prompt the user wrote',
  maxToolIterations: 12,
  agenticSampling: false,
  toolCallRepair: false,
  maxStagnationSteps: 7,
  defaultModelId: 3,
  defaultDownloadPath: '/downloads',
  inferenceDefaults: { temperature: 0.4, topP: 0.9, topK: 40 },
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
    saving: false,
    error: null,
    refresh: vi.fn(),
    save,
  }),
}));

vi.mock('../../../src/hooks/useMcpServers', () => ({
  useMcpServers: () => ({
    servers: [],
    tools: [],
    loading: false,
    error: null,
    refresh: vi.fn(),
  }),
}));

vi.mock('../../../src/hooks/useModels', () => ({
  useModels: () => ({
    models: [
      { id: 3, name: 'three' },
      { id: 4, name: 'four' },
    ],
    loading: false,
    error: null,
    refresh: vi.fn(),
  }),
}));

// The loop guard's log panel reads when the Advanced section opens.
vi.mock('../../../src/services/transport/api/proxy', () => ({
  getLoopGuardTrips: vi.fn(async () => []),
}));

const field = (id: string) => {
  const element = document.querySelector<HTMLInputElement>(`#${id}`);
  if (!element) throw new Error(`no #${id} in the dialog`);
  return element;
};

type User = ReturnType<typeof userEvent.setup>;
type Change = (user: User) => Promise<void>;

const retype = (id: string, text: string): Change => async (user) => {
  await user.clear(field(id));
  await user.type(field(id), text);
};
const toggle = (id: string): Change => async (user) => {
  await user.click(field(id));
};
const choose = (id: string, value: string): Change => async (user) => {
  await user.selectOptions(field(id), value);
};
/** A change to a field the collapsed Advanced section holds. */
const advanced = (change: Change): Change => async (user) => {
  await user.click(screen.getByRole('button', { name: /advanced settings/i }));
  await change(user);
};

/** Every field a Save can send: how to change it, and the request it then sends. */
const changes: [string, Change, UpdateSettingsRequest][] = [
  ['context size', retype('context-size-input', '4096'), { defaultContextSize: 4096 }],
  ['default model', choose('default-model-select', '4'), { defaultModelId: 4 }],
  ['proxy port', retype('proxy-port-input', '8082'), { proxyPort: 8082 }],
  ['base server port', retype('server-port-input', '9200'), { llamaBasePort: 9200 }],
  ['download queue size', retype('max-queue-size-input', '6'), { maxDownloadQueueSize: 6 }],
  ['proxy key', retype('proxy-api-key-input', 'sk-new'), { proxyApiKey: 'sk-new' }],
  ['download path', retype('download-path-input', '/elsewhere'), { defaultDownloadPath: '/elsewhere' }],
  ['bind host', retype('bind-host-input', '127.0.0.1'), { bindHost: '127.0.0.1' }],
  ['LAN sharing', toggle('share-lan-input'), { shareLan: false }],
  ['fit indicators', toggle('show-fit-indicators-input'), { showMemoryFitIndicators: true }],
  ['proxy autostart', toggle('proxy-autostart-input'), { proxyAutostart: false }],
  ['close to tray', toggle('close-to-tray-input'), { closeToTray: false }],
  ['start at login', toggle('start-at-login-input'), { startAtLogin: false }],
  ['tool iterations', advanced(retype('max-tool-iterations-input', '20')), { maxToolIterations: 20 }],
  ['title prompt', advanced(retype('title-prompt-input', 'another prompt')), { titleGenerationPrompt: 'another prompt' }],
  [
    'inference defaults',
    advanced(retype('inference-param-topK', '50')),
    { inferenceDefaults: { temperature: 0.4, topP: 0.9, topK: 50 } },
  ],
  ['client sampling trust', advanced(toggle('trust-client-sampling-input')), { trustClientSampling: false }],
  ['loop guard mode', advanced(choose('loop-guard-mode-input', 'off')), { loopGuardMode: 'off' }],
  ['agentic sampling cap', advanced(toggle('agentic-sampling-input')), { agenticSampling: true }],
  ['tool call repair', advanced(toggle('tool-call-repair-input')), { toolCallRepair: true }],
  ['stagnation limit', advanced(retype('max-stagnation-steps-input', '9')), { maxStagnationSteps: 9 }],
];

describe('SettingsModal — save', () => {
  beforeEach(() => vi.clearAllMocks());

  it.each(changes)('sends the %s alone when only it changed', async (_name, change, request) => {
    const user = userEvent.setup();
    render(<SettingsModal isOpen onClose={vi.fn()} />);

    await change(user);
    await user.click(screen.getByRole('button', { name: /save changes/i }));

    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith(request);
  });

  it('sends nothing when nothing changed', async () => {
    const user = userEvent.setup();
    render(<SettingsModal isOpen onClose={vi.fn()} />);

    await user.click(screen.getByRole('button', { name: /save changes/i }));

    expect(save).not.toHaveBeenCalled();
  });

  it('sends a field changed and changed back as nothing', async () => {
    const user = userEvent.setup();
    render(<SettingsModal isOpen onClose={vi.fn()} />);

    await user.clear(field('proxy-port-input'));
    await user.type(field('proxy-port-input'), '8081');
    await user.click(screen.getByRole('button', { name: /save changes/i }));

    expect(save).not.toHaveBeenCalled();
  });

  /** The form builds a new object for every edit, so this compares by value. */
  it('sends an inference default changed and changed back as nothing', async () => {
    const user = userEvent.setup();
    render(<SettingsModal isOpen onClose={vi.fn()} />);

    await advanced(retype('inference-param-topK', '40'))(user);
    await user.click(screen.getByRole('button', { name: /save changes/i }));

    expect(save).not.toHaveBeenCalled();
  });
});
