/**
 * A fake daemon for the settings place tests: the stored settings, the key
 * kept apart as the daemon keeps it (a read says only whether one is held),
 * the models folder, and the tool servers. `updateSettings` merges a write
 * the way `PUT /api/config/settings` does and answers what is stored; a
 * write is refused, in the daemon's words, while `refuse` is set.
 */

import { vi } from 'vitest';
import type { AppSettings, ModelsDirectoryInfo, UpdateSettingsRequest } from '../../../src/types';
import type { McpServerInfo, NewMcpServer } from '../../../src/services/transport/types/mcp';
import { mcpServer, mcpServerInfo } from '../fixtures/mcp';
import { appSettings } from '../fixtures/settings';

export const daemon = {
  settings: appSettings(),
  key: null as string | null,
  folder: { path: '/models', source: 'default', default_path: '/models', exists: true, writable: true } as ModelsDirectoryInfo,
  servers: [] as McpServerInfo[],
  refuse: null as string | null,
};

/** A fresh install with `overrides`, and `key` held if given. */
export function resetDaemon(overrides: Partial<AppSettings> = {}, key: string | null = null): void {
  daemon.settings = appSettings({ ...overrides, hasProxyApiKey: key !== null });
  daemon.key = key;
  daemon.folder = { path: '/models', source: 'default', default_path: '/models', exists: true, writable: true };
  daemon.servers = [];
  daemon.refuse = null;
  Object.values(transport).forEach((fn) => fn.mockClear());
}

export const transport = {
  getSettings: vi.fn(async () => ({ ...daemon.settings })),
  updateSettings: vi.fn(async (update: UpdateSettingsRequest): Promise<AppSettings> => {
    if (daemon.refuse !== null) throw new Error(daemon.refuse);
    const { proxyApiKey, ...rest } = update;
    if (proxyApiKey !== undefined) daemon.key = proxyApiKey;
    daemon.settings = { ...daemon.settings, ...(rest as Partial<AppSettings>), hasProxyApiKey: daemon.key !== null };
    return { ...daemon.settings };
  }),
  revealSecret: vi.fn(async () => daemon.key),
  getModelsDirectory: vi.fn(async () => ({ ...daemon.folder })),
  setModelsDirectory: vi.fn(async (path: string) => {
    if (daemon.refuse !== null) throw new Error(daemon.refuse);
    daemon.folder = { ...daemon.folder, path, source: path === daemon.folder.default_path ? 'default' : 'explicit' };
  }),
  listMcpServers: vi.fn(async () => [...daemon.servers]),
  addMcpServer: vi.fn(async (server: NewMcpServer) => {
    const info = mcpServerInfo({ server: mcpServer({ id: daemon.servers.length + 1, name: server.name }) });
    daemon.servers.push(info);
    return info;
  }),
  getVersion: vi.fn(async () => ({ display: '0.31.0 (abc1234)', sha: 'abc1234', fingerprint: 'abc1234' })),
  shutdownDaemon: vi.fn(async () => {}),
  subscribe: vi.fn(() => () => {}),
  onEventStreamOpen: vi.fn(() => () => {}),
};

/** Every settings write so far, in order. */
export const writes = (): UpdateSettingsRequest[] => transport.updateSettings.mock.calls.map(([update]) => update);
