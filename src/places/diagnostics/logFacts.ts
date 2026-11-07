/**
 * What Diagnostics › Logs says beside a loaded model's log, each fact once
 * and only when the daemon has it. Pure: the server and a dashboard frame
 * in, labelled values out.
 *
 * The port is the server list's. The rest is the frame's: the model's
 * resident slot (how long it has been loaded, the context it was launched
 * with, the requests holding it), its newest recorded rate
 * (`readings/throughput`), and, for the primary model only, the context
 * its llama.cpp slots hold, since the proxy polls the primary's alone.
 * Without the frame (the endpoint off) the port is all there is.
 *
 * @module places/diagnostics/logFacts
 */

import { latestRate, rateWords } from '../../readings/throughput';
import { tokensInUse } from '../../services/transport/slotTokens';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';
import { usagePercent } from '../../utils/contextUsage';
import { formatCount, formatDuration } from '../../utils/format';

export interface LogFact {
  label: string;
  value: string;
}

/** The context the primary's llama.cpp slots hold together, of what they have; `null` when unknown. */
function kvUse(snapshot: DashboardSnapshot): string | null {
  if (!snapshot.slots_available) return null;
  let used = 0;
  let size = 0;
  for (const slot of snapshot.slots) {
    const tokens = tokensInUse(slot);
    if (tokens === null || slot.n_ctx === null) return null;
    used += tokens;
    size += slot.n_ctx;
  }
  return size > 0 ? `${formatCount(used)} of ${formatCount(size)} tokens · ${usagePercent(used, size)}%` : null;
}

export function logFacts(server: { modelId: number; port: number }, snapshot: DashboardSnapshot | null): LogFact[] {
  const facts: LogFact[] = [{ label: 'Port', value: String(server.port) }];
  const slot = snapshot?.admission.slots.find((s) => s.model_id === server.modelId);
  if (!snapshot || !slot) return facts;
  facts.push({ label: 'Uptime', value: formatDuration(slot.resident_for_secs) });
  if (slot.launch) facts.push({ label: 'Context', value: `${formatCount(slot.launch.context_size)} tokens` });
  const kv = slot.is_primary ? kvUse(snapshot) : null;
  if (kv !== null) facts.push({ label: 'KV use', value: kv });
  const rate = latestRate(snapshot.recent_requests, slot.model_name);
  if (rate !== null) facts.push({ label: 'Writing', value: rateWords(rate) });
  facts.push({ label: 'Active requests', value: formatCount(slot.inflight) });
  return facts;
}
