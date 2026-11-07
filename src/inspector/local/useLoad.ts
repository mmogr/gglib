/**
 * The inspector's main actions: Load, Chat and Unload.
 *
 * Load is one click and sends nothing the person did not choose: the model's
 * id, and no context, port or sampling (IN-7); the daemon resolves the rest
 * as `gglib model explain` says it would. A load opens no chat. Chat on a
 * model that is not loaded loads it first, then hands the new server's port
 * to the chat (`onServerStarted`). Unload always asks the daemon (TX-12,
 * `safeStopServer` behind `onStopServer`).
 *
 * llama.cpp missing (the setup status says so, or a load is refused for
 * it) turns Load into Install llama.cpp: the library banner's install
 * (`places/library/engineInstall`), then the load (or the chat) that asked
 * for it once the install completes. A failed load or unload is one line,
 * in the daemon's words, saying which it was.
 *
 * `loadWith` is the Load with options sheet's start: the request the sheet
 * built from what was typed (`inspector/loadOptions`), after the pin when
 * asked (`POST /api/proxy/start-pinned`, which loads nothing itself), and
 * no chat. It answers the daemon's words when it fails, for the sheet to
 * keep; a refusal for want of llama.cpp also turns Load into its install.
 *
 * @module inspector/local/useLoad
 */

import { useEffect, useRef, useState } from 'react';
import { useLlamaStatus } from '../../hooks/useLlamaStatus';
import type { ServerViewModel } from '../../hooks/useServers';
import { installLlama, useEngineInstall } from '../../places/library/engineInstall';
import { getTransport } from '../../services/transport';
import { TransportError } from '../../services/transport/errors';
import { toStartServerRequest } from '../../services/transport/mappers';
import type { GgufModel, ServeConfig } from '../../types';
import { formatError } from '../../utils/errors';

type Then = 'load' | 'chat';

export interface LoadState {
  /** What is in flight: a load, a load for a chat, or an unload. */
  busy: Then | 'unload' | null;
  /** Which action last failed, and why, in the daemon's words. */
  error: { action: 'load' | 'unload'; message: string } | null;
  /** llama.cpp must be installed before this model can load. */
  needsLlama: boolean;
  load: () => void;
  /** Load, then open the chat on the new server. */
  loadForChat: () => void;
  unload: () => void;
  /** Load with what the sheet built, pinned first when `pin`; the daemon's words when it fails, else `null`. */
  loadWith: (config: ServeConfig, pin: boolean) => Promise<string | null>;
}

export function useLoad(
  model: GgufModel,
  draws: boolean,
  onServerStarted: (server: ServerViewModel) => void,
  onStopServer: (modelId: number) => Promise<void>,
): LoadState {
  const [busy, setBusy] = useState<LoadState['busy']>(null);
  const [error, setError] = useState<LoadState['error']>(null);
  const [refused, setRefused] = useState(false);
  const { status: llama, checkStatus } = useLlamaStatus();
  const install = useEngineInstall();
  const after = useRef<Then | null>(null);
  const needsLlama = !draws && (refused || llama?.installed === false);

  const start = async (then: Then) => {
    setError(null);
    setBusy(then);
    try {
      const { port } = await getTransport().serveModel({ id: model.id });
      if (then === 'chat') onServerStarted({ modelId: model.id, modelName: model.name, port, status: 'running' });
    } catch (err) {
      if (TransportError.hasCode(err, 'LLAMA_SERVER_NOT_INSTALLED')) setRefused(true);
      else setError({ action: 'load', message: formatError(err) });
    } finally {
      setBusy(null);
    }
  };

  const go = (then: Then) => {
    if (!needsLlama) return void start(then);
    after.current = then;
    installLlama();
  };

  // An install this inspector asked for completed: load as it was asked to.
  const seen = useRef(install.completed);
  useEffect(() => {
    if (install.completed === seen.current) return;
    seen.current = install.completed;
    setRefused(false);
    void checkStatus();
    const then = after.current;
    after.current = null;
    if (then) void start(then);
    // `start` is this render's; the effect runs on a completion only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [install.completed]);

  const loadWith = async (config: ServeConfig, pin: boolean): Promise<string | null> => {
    setError(null);
    setBusy('load');
    try {
      if (pin) await getTransport().startPinnedProxy({ model_id: model.id, options: toStartServerRequest(config) });
      await getTransport().serveModel(config);
      return null;
    } catch (err) {
      if (TransportError.hasCode(err, 'LLAMA_SERVER_NOT_INSTALLED')) setRefused(true);
      return formatError(err);
    } finally {
      setBusy(null);
    }
  };

  const unload = async () => {
    setError(null);
    setBusy('unload');
    try {
      await onStopServer(model.id);
    } catch (err) {
      setError({ action: 'unload', message: formatError(err) });
    } finally {
      setBusy(null);
    }
  };

  return {
    busy,
    error: error ?? (needsLlama && install.refusal ? { action: 'load', message: install.refusal } : null),
    needsLlama,
    load: () => go('load'),
    loadForChat: () => go('chat'),
    unload: () => void unload(),
    loadWith,
  };
}
