/**
 * "Pick a size": one row per variant, by its uploader's label, with its
 * size (its files on hover) and the daemon's fit verdict in the daemon's
 * own words, always shown. One row is selected, and only it carries
 * `[■ Download]`. gglib's pick (`picked`: the variant `gglib model
 * download` takes without `-q`, the daemon's choice: the listing's, else the
 * download's own pick, which the inspector asks beside it) carries ★ and is
 * selected once known; with none marked, no row is until one is chosen. The selected row is scrolled into view
 * whenever it changes. The size and the fit are the weights alone, as the
 * download's own pick judges them; the projector fetched beside them counts
 * for neither, and is said under the table for the selected row. A draft or
 * helper (its header's role, `Sidecar`) is dimmed, "not a model on its
 * own", and cannot be selected. Columns that fit a 320 px inspector.
 *
 * @module inspector/hf/PickASize
 */

import { useEffect, useRef, type FC } from 'react';
import { Star } from 'lucide-react';
import { Icon } from '../../components/ui/Icon';
import { StatusMark, type MarkKind } from '../../components/status/StatusMark';
import { Button } from '../../components/ui/Button';
import type { HfQuantization, VariantFit } from '../../types';
import { cn } from '../../utils/cn';
import { formatSize } from '../../utils/format';

/** The shape for each of the daemon's verdicts, beside its words. */
const FIT_MARK: Readonly<Record<VariantFit['kind'], MarkKind>> = {
  fits: 'on',
  tight: 'attention',
  too_big: 'failed',
  unknown: 'off',
};

/** Why a draft or helper variant offers no download. */
const HELPER_FILES_REFUSAL = "gglib can't run draft or helper files on their own";

const isModel = (q: HfQuantization) => q.role !== 'Sidecar';

/** The files a variant is, for its size's hover. */
function files(q: HfQuantization): string {
  const first = q.file_path.split('/').pop() || q.file_path;
  return q.is_sharded && q.shard_count ? `${q.shard_count} files, from ${first}` : first;
}

/** One grid for the header and every row. */
const COLUMNS = 'grid grid-cols-[minmax(0,1fr)_4.5rem_minmax(0,1.2fr)] items-baseline gap-sm';

interface PickASizeProps {
  /** The repository's variants, smallest first, no projector among them. */
  quantizations: HfQuantization[];
  selected: HfQuantization | undefined;
  onSelect: (quant: HfQuantization) => void;
  onDownload: (quant: HfQuantization) => void;
  /** The daemon would refuse another download. */
  queueFull: boolean;
}

export const PickASize: FC<PickASizeProps> = ({ quantizations, selected, onSelect, onDownload, queueFull }) => {
  const selectedRow = useRef<HTMLDivElement>(null);
  // jsdom has no scrollIntoView.
  useEffect(() => selectedRow.current?.scrollIntoView?.({ block: 'nearest' }), [selected?.label]);
  return (
    <section aria-label="Pick a size" className="flex flex-col gap-xs">
      <h3 className="m-0 text-xs font-semibold text-text">Pick a size</h3>
      <div className={cn(COLUMNS, 'px-sm text-xs text-text-secondary')} aria-hidden>
        <span>Quant</span>
        <span className="text-right">Size</span>
        <span>Fit</span>
      </div>
      <div role="listbox" aria-label="Sizes" className="flex flex-col">
        {quantizations.map((q) => {
          const model = isModel(q);
          const picked = model && q.label === selected?.label;
          return (
            <div
              key={q.label}
              ref={picked ? selectedRow : undefined}
              role="option"
              aria-selected={picked}
              aria-disabled={!model || undefined}
              tabIndex={model ? 0 : -1}
              title={model ? undefined : HELPER_FILES_REFUSAL}
              onClick={() => model && onSelect(q)}
              onFocus={() => model && onSelect(q)}
              className={cn(
                'flex flex-col gap-xs rounded-base px-sm py-xs text-sm outline-none focus-visible:ring-2 focus-visible:ring-primary',
                model ? 'cursor-pointer hover:bg-surface-hover' : 'text-text-muted',
                picked && 'bg-surface-hover',
              )}
            >
              <div className={COLUMNS}>
                <span className="flex min-w-0 items-center gap-xs font-semibold">
                  <span className="truncate" title={q.label}>
                    {q.label}
                  </span>
                  {q.picked && (
                    <span role="img" aria-label="gglib's pick" title="gglib's pick" className="inline-flex shrink-0 text-text-secondary">
                      <Icon icon={Star} size={12} />
                    </span>
                  )}
                </span>
                <span className="text-right tabular-nums" title={files(q)}>
                  {formatSize(q.size_bytes)}
                </span>
                {!model ? (
                  <span className="truncate text-xs">not a model on its own</span>
                ) : q.fit && q.fit_word ? (
                  <span className="min-w-0 truncate text-xs" title={q.fit_word}>
                    <StatusMark kind={FIT_MARK[q.fit.kind]} word={q.fit_word} />
                  </span>
                ) : (
                  <span />
                )}
              </div>
              {picked && (
                <div>
                  <Button
                    size="sm"
                    disabled={queueFull}
                    onClick={(e) => {
                      e.stopPropagation();
                      onDownload(q);
                    }}
                  >
                    Download
                  </Button>
                </div>
              )}
            </div>
          );
        })}
      </div>
      {queueFull && <p className="m-0 text-xs text-text-secondary">The download queue is full; Download waits for room.</p>}
    </section>
  );
};
