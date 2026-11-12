/**
 * The Hugging Face subject: a repo, at `/get/:owner/:name`, so back,
 * forward and reload keep it. Its `owner/name` ↗ opens it on the Hub; the
 * summary is the Hub's parameter count and whether the daemon read its
 * chat template as calling tools (its confidence on hover); "Pick a size"
 * is the daemon's listing of its variants; the projector the selected
 * variant brings and an image model's companions follow; ▸ About this
 * model holds the Hub's description, tags and counts. A repo with no model
 * variant says so in one line.
 *
 * The summary comes with the pick that opened the repo (the navigation's
 * state) or is read by the address; the listing and the tool reading are
 * read here, each dropped if the repo changed meanwhile.
 *
 * @module inspector/hf/HfInspector
 */

import { useEffect, useState, type FC } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { ChevronRight, ExternalLink, X } from 'lucide-react';
import { paths } from '../../app/paths';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useToastContext } from '../../contexts/ToastContext';
import { queueDownload } from '../../services/downloadsEvents';
import { useDownloads } from '../../services/downloadsRegistry';
import { openUrl } from '../../services/platform';
import { getTransport } from '../../services/transport';
import type { HfImagePreview, HfModelSummary, HfPickResponse, HfQuantization, ToolSupportResponse } from '../../types';
import { formatError } from '../../utils/errors';
import { formatNumber, formatParamCount, getHuggingFaceModelUrl } from '../../utils/format';
import { CompanionNote } from './CompanionNote';
import { PickASize } from './PickASize';
import { ProjectorNote } from './ProjectorNote';

/** What a pick hands the address it navigates to, so the repo is not read twice. */
export interface HfPickState {
  hf: HfModelSummary;
}

type Listing = { quants: HfQuantization[]; image: HfImagePreview | null } | { error: string };

/** One read of `repo`, its answer held only while it is still the repo asked. */
function useRead<T>(read: () => Promise<T>, onError: (err: unknown) => T | null, seed: T | null = null): T | null {
  const [answer, setAnswer] = useState<T | null>(seed);
  useEffect(() => {
    if (seed !== null) return;
    let live = true;
    read()
      .then((value) => live && setAnswer(value))
      .catch((err: unknown) => live && setAnswer(onError(err)));
    return () => {
      live = false;
    };
    // The inspector is keyed by the repo, so these are read once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return answer;
}

export const HfInspector: FC<{ repo: string }> = ({ repo }) => {
  const navigate = useNavigate();
  const location = useLocation();
  const { showToast } = useToastContext();
  const { snapshot } = useDownloads();
  const handed = (location.state as Partial<HfPickState> | null)?.hf;
  const summary = useRead<HfModelSummary | { error: string }>(
    () => getTransport().getHfModelSummary(repo),
    (err) => ({ error: formatError(err) }),
    handed?.id === repo ? handed : null,
  );
  const listing = useRead<Listing>(
    async () => {
      const response = await getTransport().getHfQuantizations(repo);
      // A projector is fetched with a model, never on its own, so it is not a row.
      const quants = response.quantizations.filter((q) => q.role !== 'Projector').sort((a, b) => a.size_bytes - b.size_bytes);
      return { quants, image: response.image ?? null };
    },
    (err) => ({ error: formatError(err) }),
  );
  // Asked beside the listing, which shows at once: the download's own pick,
  // which reads further than the listing's heads. A failure marks nothing.
  const pick = useRead<HfPickResponse | null>(() => getTransport().getHfPick(repo), () => null);
  // Optional: a failed reading says nothing about tools.
  const tools = useRead<ToolSupportResponse>(() => getTransport().getHfToolSupport(repo), () => null);
  const [picked, setPicked] = useState<string | null>(null);

  const listed = listing && 'quants' in listing ? listing.quants : [];
  // gglib's pick: the listing's, else the download's once it arrives.
  const pickLabel = listed.find((q) => q.picked)?.label ?? pick?.label ?? null;
  const quants = listed.map((q) => (q.label === pickLabel ? { ...q, picked: true } : q));
  // The row chosen, else gglib's pick.
  const selected = quants.find((q) => q.label === picked) ?? quants.find((q) => q.picked);
  const model = summary && 'id' in summary ? summary : null;

  const download = async (quant: HfQuantization) => {
    try {
      await queueDownload(repo, quant.label);
      showToast(`Download queued: ${repo}`, 'success');
    } catch (err) {
      showToast(formatError(err), 'error');
    }
  };

  const toolsTitle = tools
    ? `${Math.round(tools.confidence * 100)}% confident${tools.detected_format ? `, ${tools.detected_format} format` : ''}`
    : undefined;

  return (
    <div className="flex flex-col gap-lg p-md">
      <div className="flex flex-col gap-xs">
        <div className="flex items-center gap-xs">
          <h2 className="m-0 min-w-0 truncate font-mono text-base font-semibold text-text">{repo}</h2>
          <Button variant="ghost" size="sm" iconOnly aria-label="Open on Hugging Face" title="Open on Hugging Face" onClick={() => void openUrl(getHuggingFaceModelUrl(repo))}>
            <Icon icon={ExternalLink} size={14} />
          </Button>
          <span className="flex-1" />
          <Button variant="ghost" size="sm" iconOnly aria-label="Close" title="Close" onClick={() => navigate(paths.library())}>
            <Icon icon={X} size={14} />
          </Button>
        </div>
        <p className="m-0 text-xs text-text-secondary tabular-nums">
          {model?.parameters_b != null && formatParamCount(model.parameters_b)}
          {model?.parameters_b != null && tools?.supports_tool_calls && ' · '}
          {tools?.supports_tool_calls && <span title={toolsTitle}>calls tools</span>}
        </p>
        {summary && 'error' in summary && <p className="m-0 text-xs text-text-secondary">{summary.error}</p>}
      </div>

      {listing === null ? (
        <p className="m-0 text-xs text-text-secondary">Reading the repo's files…</p>
      ) : 'error' in listing ? (
        <p role="alert" className="m-0 text-xs text-text-secondary">{listing.error}</p>
      ) : quants.length === 0 ? (
        <p className="m-0 text-xs text-text-secondary">This repo has no GGUF files.</p>
      ) : (
        <div className="flex flex-col gap-sm">
          <PickASize quantizations={quants} selected={selected} onSelect={(q) => setPicked(q.label)} onDownload={(q) => void download(q)} queueFull={snapshot?.full ?? false} />
          {selected && <ProjectorNote quantization={selected} />}
          {listing.image && <CompanionNote image={listing.image} />}
        </div>
      )}

      {model && <About model={model} />}
    </div>
  );
};

/** ▸ About this model: the Hub's description, tags and counts, closed. */
const About: FC<{ model: HfModelSummary }> = ({ model }) => (
  <details className="group">
    <summary className="flex cursor-pointer list-none items-center gap-xs text-xs font-semibold text-text [&::-webkit-details-marker]:hidden">
      <Icon icon={ChevronRight} size={12} className="group-open:rotate-90" />
      About this model
    </summary>
    <div className="mt-sm flex flex-col gap-sm text-xs text-text-secondary">
      {model.description && <p className="m-0 leading-relaxed">{model.description}</p>}
      <p className="m-0 tabular-nums">{`${formatNumber(model.downloads)} downloads · ${formatNumber(model.likes)} likes`}</p>
      {model.tags.length > 0 && (
        <p className="m-0">{model.tags.slice(0, 10).join(' · ') + (model.tags.length > 10 ? ` · +${model.tags.length - 10} more` : '')}</p>
      )}
    </div>
  </details>
);
