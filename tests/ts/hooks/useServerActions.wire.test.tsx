/**
 * The JSON a Serve puts on the wire, from the serve modal's state to the body
 * `post` is handed: `POST /api/servers/start` for a start, and
 * `POST /api/proxy/start-pinned` before it for a pin.
 *
 * The hook, the mapper and the two API calls are the real ones; only the HTTP
 * client is replaced. `useServerActions.test.tsx` replaces the transport, so
 * it sees the `ServeConfig` and not what becomes of it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { ReactNode } from 'react';
import {
  useServerActions,
  ServerActionsConfig,
} from '../../../src/components/ModelInspectorPanel/hooks/useServerActions';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { guiModel } from '../fixtures/model';

const post = vi.fn();
vi.mock('../../../src/services/transport/api/client', () => ({
  post: (path: string, body?: unknown) => post(path, body),
  get: vi.fn(),
}));

vi.mock('../../../src/services/transport', async () => {
  const { serveModel } = await import('../../../src/services/transport/api/servers');
  const { startPinnedProxy } = await import('../../../src/services/transport/api/proxy');
  return { getTransport: () => ({ serveModel, startPinnedProxy }) };
});

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>{children}</ToastProvider>
);

const model = guiModel({ id: 7, tags: ['mtp'] });

/** The serve modal's state, with `mtp` as the two MTP fields hold it. */
function serve(mtp: Pick<ServerActionsConfig, 'mtpNMaxOverride' | 'mtpPMinOverride'>, pinProxy: boolean) {
  const config: ServerActionsConfig = {
    model,
    servers: [],
    editedName: model.name,
    editedQuantization: '',
    editedFilePath: model.filePath,
    editedInferenceDefaults: undefined,
    customContext: '',
    customPort: '',
    jinjaOverride: null,
    hasAgentTag: false,
    pinProxy,
    inferenceParams: undefined,
    editedServerDefaults: undefined,
    onStopServer: vi.fn(),
    onRemoveModel: vi.fn(),
    onUpdateModel: vi.fn().mockResolvedValue(undefined),
    setIsServing: vi.fn(),
    setIsDeleting: vi.fn(),
    closeServeModal: vi.fn(),
    closeDeleteModal: vi.fn(),
    resetEditState: vi.fn(),
    ...mtp,
  };
  return renderHook(() => useServerActions(config), { wrapper });
}

/** Each body `post` was handed, as the JSON it is sent as, by path. */
function sent(): Record<string, string> {
  return Object.fromEntries(
    (post.mock.calls as [string, unknown][]).map(([path, body]) => [path, JSON.stringify(body)]),
  );
}

describe('useServerActions handleStartServer — the JSON a serve sends', () => {
  beforeEach(() => {
    post.mockReset();
    post.mockResolvedValue({ port: 9001 });
  });

  it.each([
    {
      name: 'MTP left to the model',
      mtp: { mtpNMaxOverride: null, mtpPMinOverride: null },
      options: '{"mlock":false}',
    },
    {
      name: 'MTP set',
      mtp: { mtpNMaxOverride: 4, mtpPMinOverride: 0.6 },
      options: '{"mlock":false,"mtpDraftNMax":4,"mtpDraftPMin":0.6}',
    },
    {
      // A zero is the one value a truthiness test would drop, and it is the
      // one that says "off" to a model whose tag says "on".
      name: 'MTP turned off',
      mtp: { mtpNMaxOverride: 0, mtpPMinOverride: null },
      options: '{"mlock":false,"mtpDraftNMax":0}',
    },
  ])('$name', async ({ mtp, options }) => {
    const start = `{"id":7,${options.slice(1)}`;

    const bare = serve(mtp, false);
    await act(async () => {
      await bare.result.current.handleStartServer();
    });
    expect(sent()).toEqual({ '/api/servers/start': start });

    post.mockClear();
    const pinned = serve(mtp, true);
    await act(async () => {
      await pinned.result.current.handleStartServer();
    });
    expect(sent()).toEqual({
      '/api/proxy/start-pinned': `{"model_id":7,"options":${options}}`,
      '/api/servers/start': start,
    });
  });
});
