import { FC, useCallback, useEffect, useState } from 'react';
import { Select } from '../../ui/Select';
import { Button } from '../../ui/Button';
import type { GgufModel } from '../../../types';
import type { AgenticEvalReport } from '../../../types/benchmark';
import { getModelAgenticHistory } from '../../../services/clients/benchmark';

interface AgenticHistoryListProps {
  models: GgufModel[];
  onSelect: (report: AgenticEvalReport) => void;
}

/**
 * Past agentic reports for a chosen model, most recent first. Reports carry
 * no timestamp of their own, so entries are numbered with the newest as #1.
 */
export const AgenticHistoryList: FC<AgenticHistoryListProps> = ({ models, onSelect }) => {
  const [modelId, setModelId] = useState<number | ''>('');
  const [reports, setReports] = useState<AgenticEvalReport[]>([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async (id: number) => {
    setLoading(true);
    try {
      setReports(await getModelAgenticHistory(id));
    } catch {
      setReports([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (modelId !== '') void load(modelId);
    else setReports([]);
  }, [modelId, load]);

  return (
    <section className="flex flex-col gap-sm">
      <h3 className="m-0 text-sm font-semibold text-text">Previous reports</h3>
      <div className="flex items-center gap-sm max-w-[360px]">
        <Select
          size="sm"
          value={modelId}
          onChange={(e) => setModelId(e.target.value ? Number(e.target.value) : '')}
          aria-label="Model for previous reports"
        >
          <option value="">Pick a model…</option>
          {models.map((m) => (
            <option key={m.id} value={m.id ?? ''}>
              {m.name}
            </option>
          ))}
        </Select>
      </div>
      {loading && <p className="text-xs text-text-muted m-0">Loading…</p>}
      {!loading && modelId !== '' && reports.length === 0 && (
        <p className="text-xs text-text-muted m-0">No past reports for this model.</p>
      )}
      {reports.map((r, i) => (
        <Button
          key={i}
          variant="secondary"
          size="sm"
          className="self-start font-mono tabular-nums"
          onClick={() => onSelect(r)}
        >
          #{i + 1}
          {i === 0 ? ' (latest)' : ''} · Δ composite{' '}
          {/* A withheld delta must not read as a small one, least of all in a
              list where past runs are scanned side by side. */}
          {r.delta.composite == null
            ? 'withheld'
            : `${r.delta.composite >= 0 ? '+' : ''}${r.delta.composite.toFixed(3)}`}
        </Button>
      ))}
    </section>
  );
};
