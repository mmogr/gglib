/**
 * What the endpoint place does to the endpoint: turn it on or off, serve
 * every model or only one, and test it.
 *
 * Turn on names nothing, not even a port (EP-2): the daemon starts the
 * endpoint as the settings say, the same start the tray and `gglib proxy`
 * get. Serving only one model, or every model again, restarts it: the
 * daemon answers a start while it runs with a refusal, so the endpoint is
 * stopped first, then started pinned (`start-pinned`, the GUI's
 * `gglib serve`) or plain. A failed call leaves the daemon's message
 * verbatim (EP-1); nothing here rewords it.
 *
 * @module places/endpoint/useEndpointActions
 */

import { useCallback, useState } from 'react';
import { proofAnswered, proofFailed } from '../../readings/connect';
import { getTransport } from '../../services/transport';
import { formatError } from '../../utils/errors';

export type EndpointBusy = 'on' | 'off' | 'serve' | null;

/** The proof's outcome as the page says it. */
export interface Proof {
  answered: boolean;
  sentence: string;
}

export interface EndpointActions {
  busy: EndpointBusy;
  /** The daemon's message from the last call that failed, as it said it; `null` after one that worked. */
  error: string | null;
  turnOn: () => Promise<void>;
  turnOff: () => Promise<void>;
  /** Serve only `modelId`, or every model with `null`: a restart either way. */
  serve: (modelId: number | null) => Promise<void>;
  /** Whether a test is in flight, and what the last one found. */
  testing: boolean;
  proof: Proof | null;
  /** Send one real request for `modelId` (the pin when `null`). */
  test: (modelId: number | null) => Promise<void>;
}

export function useEndpointActions(): EndpointActions {
  const [busy, setBusy] = useState<EndpointBusy>(null);
  const [error, setError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [proof, setProof] = useState<Proof | null>(null);

  const run = useCallback(async (what: Exclude<EndpointBusy, null>, call: () => Promise<unknown>) => {
    setBusy(what);
    setError(null);
    setProof(null);
    try {
      await call();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setBusy(null);
    }
  }, []);

  const turnOn = useCallback(() => run('on', () => getTransport().startProxy({})), [run]);
  const turnOff = useCallback(() => run('off', () => getTransport().stopProxy()), [run]);
  const serve = useCallback(
    (modelId: number | null) =>
      run('serve', async () => {
        const transport = getTransport();
        await transport.stopProxy();
        if (modelId === null) await transport.startProxy({});
        else await transport.startPinnedProxy({ model_id: modelId });
      }),
    [run],
  );

  const test = useCallback(async (modelId: number | null) => {
    setTesting(true);
    setProof(null);
    try {
      setProof({ answered: true, sentence: proofAnswered(await getTransport().proveProxy(modelId)) });
    } catch (err) {
      setProof({ answered: false, sentence: proofFailed(formatError(err)) });
    } finally {
      setTesting(false);
    }
  }, []);

  return { busy, error, turnOn, turnOff, serve, testing, proof, test };
}
