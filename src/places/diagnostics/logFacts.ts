/**
 * What Diagnostics › Logs says beside a loaded model's log, each fact once
 * and only when the daemon has it. Pure: the server and a dashboard frame
 * in, labelled values out.
 *
 * The port is the server list's. The rest is the frame's: the model's
 * resident slot (how long it has been loaded, the context it was launched
 * with, the requests holding it), its newest recorded rate
 * (the proxy's, `readings/rate`), and, for the primary model only, the context
 * each of its llama.cpp slots holds, slot by slot as `gglib proxy
 * dashboard` prints them, since the proxy polls the primary's alone.
 * Without the frame (the endpoint off) the port is all there is.
 *
 * @module places/diagnostics/logFacts
 */

import { latestRate, rateWords } from '../../readings/rate';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';
import { formatCount } from '../../utils/format';
import { slotContext, slotContextWords } from './live/slotContext';

export interface LogFact {
  label: string;
  value: string;
}

export function logFacts(server: { modelId: number; port: number }, snapshot: DashboardSnapshot | null): LogFact[] {
  const facts: LogFact[] = [{ label: 'Port', value: String(server.port) }];
  const slot = snapshot?.admission.slots.find((s) => s.model_id === server.modelId);
  if (!snapshot || !slot) return facts;
  facts.push({ label: 'Uptime', value: slot.resident_for });
  if (slot.launch) facts.push({ label: 'Context', value: `${formatCount(slot.launch.context_size)} tokens` });
  // Slot by slot, as the CLI prints them, and only for the primary: the
  // proxy polls no other model's slots.
  if (slot.is_primary) {
    for (const s of snapshot.slots) {
      const context = slotContext(s);
      if (context) facts.push({ label: `KV use · slot ${s.id}`, value: slotContextWords(context) });
    }
  }
  const rate = latestRate(snapshot.recent_requests, slot.model_name);
  if (rate !== null) facts.push({ label: 'Writing', value: rateWords(rate) });
  facts.push({ label: 'Active requests', value: formatCount(slot.inflight) });
  return facts;
}
