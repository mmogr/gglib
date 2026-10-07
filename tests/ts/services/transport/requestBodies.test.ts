/**
 * What the typed request bodies put on the wire: the method, the path and the
 * JSON, key for key.
 *
 * Each body is typed by the binding generated from its Rust request struct,
 * which checks its keys against that struct and nothing else. What a typed
 * body cannot say is which keys are left out, and the daemon reads a missing
 * key and a `null` differently where the field is a `double_option`: a
 * conversation's `system_prompt`, a model's `serverDefaults` and
 * `projectorPath`. So these read the JSON itself.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import {
  updateConversationSystemPrompt,
  updateConversationTitle,
} from '../../../../src/services/transport/api/chat';
import { addMcpServer, updateMcpServer } from '../../../../src/services/transport/api/mcp';
import {
  addModel,
  removeModel,
  retagModel,
  setModelsDirectory,
  updateModel,
} from '../../../../src/services/transport/api/models/local';
import { killRemote } from '../../../../src/services/transport/api/remote';
import { stopServer } from '../../../../src/services/transport/api/servers';
import { installProfileTemplates } from '../../../../src/services/transport/api/settings';
import { addModelTag } from '../../../../src/services/transport/api/tags';
import { repairModel } from '../../../../src/services/transport/api/verification';

const fetchMock = vi.fn();

/** What one call sent: its method, its path and its body as the daemon parses it. */
async function sent(call: () => Promise<unknown>): Promise<{ method: string; path: string; body: unknown }> {
  fetchMock.mockResolvedValueOnce(
    new Response('{}', { status: 200, headers: { 'content-type': 'application/json' } }),
  );
  await call();
  expect(fetchMock).toHaveBeenCalledTimes(1);
  const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit];
  return { method: init.method ?? 'GET', path, body: JSON.parse(init.body as string) as unknown };
}

describe('what a typed request body sends', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('installing the starter profiles posts to the install route and names none', async () => {
    expect(await sent(() => installProfileTemplates())).toStrictEqual({
      method: 'POST',
      path: '/api/config/profiles/install-templates',
      body: null,
    });
  });

  it('adding a model sends its file path and nothing else', async () => {
    expect(await sent(() => addModel({ filePath: '/models/x.gguf' }))).toStrictEqual({
      method: 'POST',
      path: '/api/models',
      body: { file_path: '/models/x.gguf' },
    });
  });

  it('removing a model says it is not forced', async () => {
    expect(await sent(() => removeModel(7))).toStrictEqual({
      method: 'DELETE',
      path: '/api/models/7',
      body: { force: false },
    });
  });

  it('updating a model sends only the fields it names', async () => {
    expect(await sent(() => updateModel({ id: 7, name: 'renamed' }))).toStrictEqual({
      method: 'PUT',
      path: '/api/models/7',
      body: { name: 'renamed' },
    });
  });

  it('updating a model sends every field in camelCase, a null as a null', async () => {
    const update = {
      id: 7,
      name: 'renamed',
      quantization: 'Q4_K_M',
      filePath: '/models/y.gguf',
      inferenceDefaults: { temperature: 0.2 },
      serverDefaults: null,
      projectorPath: null,
    };
    expect(await sent(() => updateModel(update))).toStrictEqual({
      method: 'PUT',
      path: '/api/models/7',
      body: {
        name: 'renamed',
        quantization: 'Q4_K_M',
        filePath: '/models/y.gguf',
        inferenceDefaults: { temperature: 0.2 },
        serverDefaults: null,
        projectorPath: null,
      },
    });
  });

  it('retagging a model says whether the pass is full', async () => {
    expect(await sent(() => retagModel(7))).toStrictEqual({
      method: 'POST',
      path: '/api/models/7/retag',
      body: { full: false },
    });
    fetchMock.mockReset();
    expect((await sent(() => retagModel(7, true))).body).toStrictEqual({ full: true });
  });

  it('setting the models directory sends the path', async () => {
    expect(await sent(() => setModelsDirectory('/models'))).toStrictEqual({
      method: 'PUT',
      path: '/api/config/system/models-directory',
      body: { path: '/models' },
    });
  });

  it('adding a tag sends the tag', async () => {
    expect(await sent(() => addModelTag(7, 'coding'))).toStrictEqual({
      method: 'POST',
      path: '/api/models/7/tags',
      body: { tag: 'coding' },
    });
  });

  it('stopping a server names the model by `model_id`', async () => {
    expect(await sent(() => stopServer(7))).toStrictEqual({
      method: 'POST',
      path: '/api/servers/stop',
      body: { model_id: 7 },
    });
  });

  it('repairing a model with no shards named sends no `shards` key', async () => {
    expect(await sent(() => repairModel(7))).toStrictEqual({
      method: 'POST',
      path: '/api/models/7/repair',
      body: {},
    });
  });

  it('repairing named shards sends them', async () => {
    expect((await sent(() => repairModel(7, [0, 2]))).body).toStrictEqual({ shards: [0, 2] });
  });

  it("stopping the far machine's daemon sends the confirmation word", async () => {
    expect(await sent(() => killRemote())).toStrictEqual({
      method: 'POST',
      path: '/api/remote/kill',
      body: { confirm: 'shutdown' },
    });
  });

  it('renaming a conversation sends the title and no `system_prompt` key', async () => {
    expect(await sent(() => updateConversationTitle(5, 'Why the build broke'))).toStrictEqual({
      method: 'PUT',
      path: '/api/conversations/5',
      body: { title: 'Why the build broke' },
    });
  });

  it("clearing a conversation's system prompt sends a null, and no `title` key", async () => {
    expect(await sent(() => updateConversationSystemPrompt(5, null))).toStrictEqual({
      method: 'PUT',
      path: '/api/conversations/5',
      body: { system_prompt: null },
    });
  });

  it("setting a conversation's system prompt sends it", async () => {
    expect((await sent(() => updateConversationSystemPrompt(5, 'Be brief.'))).body).toStrictEqual({
      system_prompt: 'Be brief.',
    });
  });

  it('adding an MCP server flattens its config, leaves an empty string out and sends each variable as `{key, value}`', async () => {
    const server = {
      name: 'files',
      server_type: 'stdio' as const,
      config: { command: 'npx', args: ['-y', 'server-files'], working_dir: '', path_extra: '/opt/bin' },
      enabled: true,
      lifecycle: 'lazy' as const,
      env: [{ key: 'ROOT', value: '/srv' }],
    };
    expect(await sent(() => addMcpServer(server))).toStrictEqual({
      method: 'POST',
      path: '/api/mcp/servers',
      body: {
        name: 'files',
        server_type: 'stdio',
        command: 'npx',
        args: ['-y', 'server-files'],
        path_extra: '/opt/bin',
        env: [{ key: 'ROOT', value: '/srv' }],
        lifecycle: 'lazy',
      },
    });
  });

  it('adding an MCP server with no arguments sends an empty list', async () => {
    const server = {
      name: 'events',
      server_type: 'sse' as const,
      config: { url: 'http://127.0.0.1:9000/sse' },
      enabled: true,
      lifecycle: 'eager' as const,
      env: [],
    };
    expect((await sent(() => addMcpServer(server))).body).toStrictEqual({
      name: 'events',
      server_type: 'sse',
      args: [],
      url: 'http://127.0.0.1:9000/sse',
      env: [],
      lifecycle: 'eager',
    });
  });

  it('updating an MCP server sends only what changed, each variable as `{key, value}`', async () => {
    const changes = { name: 'files', config: { command: 'npx' }, env: [{ key: 'ROOT', value: '/srv' }] };
    expect(await sent(() => updateMcpServer(3, changes))).toStrictEqual({
      method: 'PUT',
      path: '/api/mcp/servers/3',
      body: { name: 'files', command: 'npx', env: [{ key: 'ROOT', value: '/srv' }] },
    });
  });

  it('switching an MCP server off sends `enabled` alone', async () => {
    expect((await sent(() => updateMcpServer(3, { enabled: false }))).body).toStrictEqual({ enabled: false });
  });
});
