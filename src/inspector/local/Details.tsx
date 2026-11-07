/**
 * Details ▸, closed until opened and remembered per viewer: the file (path
 * ⧉, size, Change path…), where it came from (repo ↗, the revision with its
 * full ref on hover, when it was downloaded and its files last checked,
 * read as UTC through `parseDbTimestamp`) and the line a file check or an
 * update check writes there (`SourceStatus`; ⋯ Verify files and Check for
 * updates open Details and bring Source into view), the name and quant label (click
 * to edit, one field per save), architecture, trained context, tags,
 * abilities, whether the template reads reasoning effort, measured speed
 * when a benchmark exists, and the raw GGUF metadata. A `dl` with a fixed
 * label column.
 *
 * @module inspector/local/Details
 */

import { useEffect, useRef, useState, type FC, type ReactNode } from 'react';
import { Check, ChevronRight, ExternalLink } from 'lucide-react';
import { CopyValue } from '../../components/CopyValue';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { Input } from '../../components/ui/Input';
import { useToastContext } from '../../contexts/ToastContext';
import { openUrl } from '../../services/platform';
import type { GgufModel, ModelDetail } from '../../types';
import { cn } from '../../utils/cn';
import { parseDbTimestamp } from '../../utils/dbTimestamp';
import { formatError } from '../../utils/errors';
import { UNKNOWN, formatSize, getHuggingFaceUrl } from '../../utils/format';
import { Abilities } from './Abilities';
import type { Update } from './Change';
import { ReasoningSupport } from './ReasoningSupport';
import { tokens } from './rows';
import { SourceStatus } from './SourceStatus';
import { Tags } from './Tags';
import type { ModelDetailState } from './useModelDetail';
import type { Verification } from './useVerification';

const OPEN_KEY = 'gglib.inspector.details';

function remembered(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) === 'open';
  } catch {
    return false;
  }
}

function remember(open: boolean): void {
  try {
    localStorage.setItem(OPEN_KEY, open ? 'open' : 'closed');
  } catch {
    // A viewer without storage gets a closed fold each time.
  }
}

const days = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

/** A database time as "6 days ago", the exact time on hover. */
const When: FC<{ at: string; before?: string }> = ({ at, before }) => {
  const date = parseDbTimestamp(at);
  const ago = Math.round((date.getTime() - Date.now()) / 86_400_000);
  return <span title={date.toLocaleString()}>{`${before ?? ''}${days.format(ago, 'day')}`}</span>;
};

/**
 * A value that turns into a field on click and saves on Enter. With
 * `action`, the value stays as text and a separate link opens the field.
 */
const Editable: FC<{ label: string; value: string; onSave: (value: string) => Promise<void>; action?: string }> = ({ label, value, onSave, action }) => {
  const [draft, setDraft] = useState<string | null>(null);
  if (draft === null)
    return (
      <Button
        variant="link"
        size="sm"
        className={cn('h-auto justify-start p-0 text-left text-xs font-normal', action ? 'text-text-secondary' : 'text-text')}
        onClick={() => setDraft(value)}
        aria-label={`Change ${label}`}
      >
        {action ?? (value || UNKNOWN)}
      </Button>
    );
  return (
    <Input
      size="sm"
      autoFocus
      aria-label={label}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Escape') setDraft(null);
        if (e.key === 'Enter' && draft.trim() && draft !== value) void onSave(draft.trim()).then(() => setDraft(null));
      }}
      onBlur={() => setDraft(null)}
    />
  );
};

const Item: FC<{ label: string; children: ReactNode }> = ({ label, children }) => (
  <>
    <dt className="text-xs text-text-secondary">{label}</dt>
    <dd className="m-0 min-w-0 break-words text-xs text-text">{children}</dd>
  </>
);

interface DetailsProps {
  model: GgufModel;
  detail: ModelDetailState;
  loaded: boolean;
  onUpdateModel: Update;
  /** A saved field or tag may change the launch: read it again. */
  onChanged: () => void;
  verification: Verification;
  /** Bumped to open Details and bring Source into view. */
  reveal: number;
}

export const Details: FC<DetailsProps> = ({ model, detail, loaded, onUpdateModel, onChanged, verification, reveal }) => {
  const { showToast } = useToastContext();
  const [open, setOpen] = useState(remembered);
  const source = useRef<HTMLDivElement>(null);
  const shown = useRef(0);
  useEffect(() => {
    if (reveal > 0) setOpen(true);
  }, [reveal]);
  useEffect(() => {
    if (!open || reveal === shown.current) return;
    shown.current = reveal;
    // jsdom has no scrollIntoView.
    source.current?.scrollIntoView?.({ block: 'nearest' });
  }, [open, reveal]);
  const d: ModelDetail | null = detail.modelDetail?.id === model.id ? detail.modelDetail : null;
  // The daemon stores no time for a file check, so one answered here is the time it names; the stored time otherwise.
  const { status } = verification;
  const report = status.kind === 'verified' ? status.report : null;
  const checkedAt = report?.verified_at ?? d?.lastUpdateCheck;
  const save = (updates: Parameters<Update>[1]) =>
    onUpdateModel(model.id, updates)
      .then(onChanged)
      .catch((err: unknown) => showToast(formatError(err), 'error'));
  const repo = getHuggingFaceUrl(model.hfRepoId);
  const metadata = Object.entries(d?.metadata ?? {}).sort(([a], [b]) => a.localeCompare(b));
  const speed = model.benchmarkSummary?.best_tg_tps;

  return (
    <details
      open={open}
      onToggle={(e) => {
        const now = e.currentTarget.open;
        setOpen(now);
        remember(now);
      }}
      className="group"
    >
      <summary className="flex cursor-pointer list-none items-center gap-xs text-xs font-semibold text-text [&::-webkit-details-marker]:hidden">
        <Icon icon={ChevronRight} size={12} className="group-open:rotate-90" />
        Details
      </summary>
      <dl className="mt-sm grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-md gap-y-sm">
        <Item label="File">
          <span className="flex items-start gap-xs">
            <span className="min-w-0 flex-1 break-all text-left font-mono">{model.filePath}</span>
            <CopyValue value={model.filePath} label="Copy the path" onCopied={() => showToast('Copied the path', 'success')} />
          </span>
          <span className="flex items-baseline gap-sm text-text-secondary">
            {model.sizeBytes > 0 && formatSize(model.sizeBytes)}
            <Editable label="path" action="Change path…" value={model.filePath} onSave={(filePath) => save({ filePath })} />
          </span>
        </Item>
        {(model.hfRepoId || d?.revision || d?.downloadDate || status.kind !== 'idle') && (
          <Item label="Source">
            {model.hfRepoId && repo && (
              <Button variant="link" size="sm" className="font-mono text-xs font-normal" rightIcon={<Icon icon={ExternalLink} size={12} />} onClick={() => openUrl(repo)}>
                {model.hfRepoId}
              </Button>
            )}
            <span className="block text-text-secondary">
              {d?.revision && (
                <span className="font-mono" title={d.revision.value}>
                  {d.revision.short}
                </span>
              )}
              {d?.revision && d.downloadDate && ' · '}
              {d?.downloadDate && <When at={d.downloadDate} before="downloaded " />}
              {(d?.revision || d?.downloadDate) && checkedAt && ' · '}
              {checkedAt && <When at={checkedAt} before="files checked " />}
              {report?.overall_health === 'healthy' && (
                <Icon icon={Check} size={12} className="ml-xs align-middle text-success" role="img" aria-hidden={false} aria-label="intact" />
              )}
            </span>
            <div ref={source} className={status.kind === 'idle' ? undefined : 'mt-xs'}>
              <SourceStatus v={verification} />
            </div>
          </Item>
        )}
        <Item label="Name">
          <Editable label="name" value={model.name} onSave={(name) => save({ name })} />
        </Item>
        <Item label="Quant label">
          <Editable label="quant label" value={model.quantization ?? ''} onSave={(quantization) => save({ quantization })} />
          {model.bitsPerWeight != null && <span className="ml-sm text-text-secondary">{`${model.bitsPerWeight.toFixed(2)} bpw`}</span>}
        </Item>
        {model.architecture && <Item label="Architecture">{model.architecture}</Item>}
        {model.contextLength != null && <Item label="Trained context">{tokens(model.contextLength)}</Item>}
        <Item label="Tags">
          <Tags
            tags={detail.tags}
            draft={detail.newTagInput}
            onDraft={detail.setNewTagInput}
            onAdd={() => void detail.addTag().then(onChanged)}
            onRemove={(tag) => void detail.removeTag(tag).then(onChanged)}
          />
        </Item>
        <Item label="Abilities">
          <Abilities modelId={model.id} capabilities={d?.capabilities} onChanged={() => void detail.reload()} onError={(m) => showToast(m, 'error')} />
        </Item>
        <Item label="Reasoning effort">
          <ReasoningSupport support={d?.reasoningEffortSupport} isRunning={loaded} onRecheck={() => void detail.reload()} isRechecking={detail.isLoading} />
        </Item>
        {speed != null && <Item label="Measured speed">{`${speed.toFixed(1)} tok/s`}</Item>}
      </dl>
      {metadata.length > 0 && (
        <details className="group/raw mt-sm">
          <summary className="flex cursor-pointer list-none items-center gap-xs text-xs text-text-secondary [&::-webkit-details-marker]:hidden">
            <Icon icon={ChevronRight} size={12} className="group-open/raw:rotate-90" />
            {`Raw GGUF metadata (${metadata.length} keys)`}
          </summary>
          <dl className="mt-xs grid max-h-64 grid-cols-[minmax(0,45%)_1fr] gap-x-md gap-y-xs overflow-y-auto">
            {metadata.map(([key, value]) => (
              <Item key={key} label={key}>
                <span className="font-mono">{value}</span>
              </Item>
            ))}
          </dl>
        </details>
      )}
    </details>
  );
};
