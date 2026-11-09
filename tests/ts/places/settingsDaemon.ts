/**
 * A fake daemon for the settings place tests: the stored settings, the key
 * kept apart as the daemon keeps it (a read says only whether one is held),
 * the models folder, and the tool servers. `updateSettings` merges a write
 * the way `PUT /api/config/settings` does and answers what is stored; a
 * write is refused, in the daemon's words, while `refuse` is set. The tool
 * server routes keep `servers` as the daemon would: an add stored as sent
 * (its variables as keys), an edit merged, a start running with `tools`, a
 * test answering `tested`, a path resolution answering `resolution` and
 * marking the server valid when it succeeds; a start held while
 * `startHold` is set, a list read refused while `refuseList` is.
 */

import { vi } from 'vitest';
import type { AppSettings, ModelsDirectoryInfo, UpdateSettingsRequest } from '../../../src/types';
import type {
  McpServerInfo,
  McpTestResult,
  NewMcpServer,
  ResolutionStatus,
  UpdateMcpServer,
} from '../../../src/services/transport/types/mcp';
import { mcpServer, mcpServerInfo } from '../fixtures/mcp';
import { appSettings } from '../fixtures/settings';

export const daemon = {
  settings: appSettings(),
  key: null as string | null,
  folder: { path: '/models', source: 'default', default_path: '/models', exists: true, writable: true, state: 'writable' } as ModelsDirectoryInfo,
  servers: [] as McpServerInfo[],
  /** What a started server offers. */
  tools: [] as McpServerInfo['tools'],
  /** What a test answers; while `null`, a test waits until it is set by `answerTest`. */
  tested: { ok: true, error: null, tools: [] } as McpTestResult | null,
  resolution: { success: true, resolved_path: '/usr/local/bin/npx', attempts: [], warnings: [] } as ResolutionStatus,
  /** While set, a tool server start waits for it. */
  startHold: null as Promise<void> | null,
  /** While set, a read of the tool servers is refused in these words. */
  refuseList: null as string | null,
  refuse: null as string | null,
  /** While set, a settings write waits for it before it lands. */
  hold: null as Promise<void> | null,
};

/** A fresh install with `overrides`, and `key` held if given. */
export function resetDaemon(overrides: Partial<AppSettings> = {}, key: string | null = null): void {
  daemon.settings = appSettings({ ...overrides, hasProxyApiKey: key !== null });
  daemon.key = key;
  daemon.folder = { path: '/models', source: 'default', default_path: '/models', exists: true, writable: true, state: 'writable' };
  daemon.servers = [];
  daemon.tools = [];
  daemon.tested = { ok: true, error: null, tools: [] };
  daemon.resolution = { success: true, resolved_path: '/usr/local/bin/npx', attempts: [], warnings: [] };
  testWaiters.length = 0;
  daemon.startHold = null;
  daemon.refuseList = null;
  daemon.refuse = null;
  daemon.hold = null;
  Object.values(transport).forEach((fn) => fn.mockClear());
}

export const transport = {
  getSettings: vi.fn(async () => ({ ...daemon.settings })),
  updateSettings: vi.fn(async (update: UpdateSettingsRequest): Promise<AppSettings> => {
    if (daemon.hold !== null) await daemon.hold;
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
  listMcpServers: vi.fn(async () => {
    if (daemon.refuseList !== null) throw new Error(daemon.refuseList);
    return daemon.servers.map((info) => ({ ...info }));
  }),
  addMcpServer: vi.fn(async (server: NewMcpServer) => {
    if (daemon.refuse !== null) throw new Error(daemon.refuse);
    const { env, ...rest } = server;
    const id = Math.max(0, ...daemon.servers.map((i) => i.server.id)) + 1;
    const info = mcpServerInfo({ server: mcpServer({ ...rest, id, env_keys: env.map((e) => e.key) }) });
    daemon.servers.push(info);
    return info;
  }),
  updateMcpServer: vi.fn(async (id: number, update: UpdateMcpServer) => {
    if (daemon.refuse !== null) throw new Error(daemon.refuse);
    const { env, ...rest } = update;
    patchServer(id, (info) => ({
      ...info,
      server: { ...info.server, ...rest, ...(env ? { env_keys: env.map((e) => e.key) } : {}) } as McpServerInfo['server'],
    }));
    return found(id);
  }),
  removeMcpServer: vi.fn(async (id: number) => {
    daemon.servers = daemon.servers.filter((info) => info.server.id !== id);
  }),
  startMcpServer: vi.fn(async (id: number) => {
    if (daemon.startHold !== null) await daemon.startHold;
    if (daemon.refuse !== null) throw new Error(daemon.refuse);
    patchServer(id, (info) => ({ ...info, status: 'running', state: 'running', tools: daemon.tools }));
    return found(id);
  }),
  stopMcpServer: vi.fn(async (id: number) => {
    patchServer(id, (info) => ({ ...info, status: 'stopped', state: 'stopped', tools: [] }));
    return found(id);
  }),
  testMcpServer: vi.fn(async (): Promise<McpTestResult> => {
    if (daemon.tested !== null) return daemon.tested;
    return new Promise<McpTestResult>((resolve) => testWaiters.push(resolve));
  }),
  resolveMcpServerPath: vi.fn(async (id: number) => {
    if (daemon.resolution.success) patchServer(id, (info) => ({ ...info, server: { ...info.server, is_valid: true } }));
    return daemon.resolution;
  }),
  setMcpServerRemote: vi.fn(async (id: number, allow: boolean) => {
    if (daemon.refuse !== null) throw new Error(daemon.refuse);
    patchServer(id, (info) => ({ ...info, server: { ...info.server, allow_remote: allow } }));
    return found(id).server;
  }),
  getVersion: vi.fn(async () => ({ display: '0.31.0 (abc1234)', sha: 'abc1234', fingerprint: 'abc1234' })),
  shutdownDaemon: vi.fn(async () => {}),
  subscribe: vi.fn(() => () => {}),
  onEventStreamOpen: vi.fn(() => () => {}),
};

/** Tests waiting for an answer, while `daemon.tested` is `null`. */
const testWaiters: ((result: McpTestResult) => void)[] = [];

/** Answer every waiting test with `result`. */
export function answerTest(result: McpTestResult): void {
  testWaiters.splice(0).forEach((resolve) => resolve(result));
}

function patchServer(id: number, change: (info: McpServerInfo) => McpServerInfo): void {
  daemon.servers = daemon.servers.map((info) => (info.server.id === id ? change(info) : info));
}

function found(id: number): McpServerInfo {
  const info = daemon.servers.find((i) => i.server.id === id);
  if (!info) throw new Error(`no server ${id}`);
  return info;
}

/** Hold every settings write until the returned function is called. */
export function holdWrites(): () => void {
  let release = () => {};
  daemon.hold = new Promise<void>((resolve) => {
    release = () => {
      daemon.hold = null;
      resolve();
    };
  });
  return release;
}

/** Every settings write so far, in order. */
export const writes = (): UpdateSettingsRequest[] => transport.updateSettings.mock.calls.map(([update]) => update);
