/**
 * The paired machine's models, for the library and for a chat on one of
 * them: read while that machine is reached, kept when it is not.
 *
 * Read through this machine's daemon (`/api/remote/models`) while the status
 * says a machine is connected and not away, and again when it comes back, on
 * a new connection, when the window regains focus, and when the caller asks
 * (after a Load). A caller that does not need them (`enabled` false) reads
 * nothing, at none of those, and is given no rows. A read that fails, or a
 * machine that goes away, keeps the last rows, marked stale; a disconnection
 * clears them, and so does a status that names another machine than the one
 * they came from (`stillPaired`).
 * A new connection that names no machine yet (the placeholder `remote_joined`
 * writes) keeps them too, marked stale: the daemon acts on whichever machine
 * answered, where their ids may be other models.
 *
 * This machine's library is `useModels`, which this never touches: a far read
 * that hangs or fails cannot blank or delay a local row. Profile variants are
 * left out — a variant is its base model asked for another way, and the page
 * picks no far profiles.
 *
 * @module usePairedModels
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { pairedName, stillPaired, useRemoteState } from '../services/remoteRegistry';
import { getTransport } from '../services/transport';
import type { Machine } from '../types/generated/Machine';
import type { ModelAction } from '../types/generated/ModelAction';
import type { ModelInfo } from '../types/generated/ModelInfo';

/** The paired machine's models as the library shows them. */
export interface PairedGroup {
  /** The machine they came from, by its fingerprint: compared, never shown. */
  machine: Machine;
  /** What may be done to a model there. */
  actions: ModelAction[];
  /** One row per model, in that machine's order. */
  models: ModelInfo[];
}

/**
 * Whether the rows are that machine's now: `reached`; the last ones read
 * while it is away, `away`; or the last ones read before a read failed, or
 * before a new connection named the machine that answered, `stale`.
 */
export type PairedReach = 'reached' | 'away' | 'stale';

export interface PairedModelsState {
  /** The rows last read, or `null` with nothing connected or nothing read yet. */
  group: PairedGroup | null;
  /** The name the machine is shown by. */
  name: string;
  reach: PairedReach;
  /** Read the rows again now. */
  refetch: () => void;
}

export function usePairedModels(enabled = true): PairedModelsState {
  const { status } = useRemoteState();
  const connected = status?.connected ?? null;
  const away = connected?.away_for_s != null;
  const readable = enabled && connected !== null && !away;
  const [group, setGroup] = useState<PairedGroup | null>(null);
  const [failed, setFailed] = useState(false);
  // Which read is the newest: one that lands after a newer one, or after a
  // disconnection, is dropped.
  const generation = useRef(0);

  const read = useCallback(async () => {
    const asked = ++generation.current;
    try {
      const paired = await getTransport().listPairedModels();
      if (asked !== generation.current) return;
      setGroup({ ...paired, models: paired.models.filter((m) => m.profile == null) });
      setFailed(false);
    } catch {
      if (asked === generation.current) setFailed(true);
    }
  }, []);

  // A new connection, the machine back, or another machine answering: read.
  // Nothing connected: clear, and drop any read still out.
  const isConnected = connected !== null;
  const peer = connected?.ticket_fingerprint ?? null;
  useEffect(() => {
    if (!isConnected) {
      generation.current++;
      setGroup(null);
      setFailed(false);
      return;
    }
    if (readable) void read();
  }, [isConnected, peer, readable, read]);

  useEffect(() => {
    if (!readable) return;
    const onFocus = () => void read();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [readable, read]);

  const refetch = useCallback(() => {
    if (readable) void read();
  }, [readable, read]);

  const held = enabled && group !== null && stillPaired(group.machine, status) ? group : null;
  const unnamed = connected !== null && connected.ticket_fingerprint === '';
  const reach: PairedReach = away ? 'away' : failed || unnamed ? 'stale' : 'reached';
  return { group: held, name: pairedName(status), reach, refetch };
}
