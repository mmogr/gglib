/**
 * Details ▸ for the paired machine's model, closed or open as this viewer
 * last left the local model's: the facts that machine exposes (size,
 * architecture, quant with its bits per weight, trained context, where it
 * came from: repo ↗, revision short with the full value on hover, when it
 * was downloaded) and the command that does the same from a terminal here,
 * ⧉. That machine's disk path is not this one's to show.
 *
 * @module inspector/far/FarDetails
 */

import { useState, type FC } from 'react';
import { ChevronRight, ExternalLink } from 'lucide-react';
import { CopyValue } from '../../components/CopyValue';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useToastContext } from '../../contexts/ToastContext';
import { openUrl } from '../../services/platform';
import type { ModelDetailDto } from '../../types/generated/ModelDetailDto';
import { formatParamCount, getHuggingFaceUrl } from '../../utils/format';
import { Item, When, remember, remembered } from '../local/Details';
import { tokens } from '../../readings/launchFit';

export const FarDetails: FC<{ id: number; detail: ModelDetailDto }> = ({ id, detail: d }) => {
  const { showToast } = useToastContext();
  const [open, setOpen] = useState(remembered);
  const repo = getHuggingFaceUrl(d.hfRepoId);
  const command = `gglib chat ${id} --remote`;
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
        <Item label="Size">{formatParamCount(d.paramCountB, d.expertUsedCount, d.expertCount)}</Item>
        {d.architecture && <Item label="Architecture">{d.architecture}</Item>}
        {(d.quantization || d.bitsPerWeight != null) && (
          <Item label="Quant">
            {[d.quantization, d.bitsPerWeight != null && `${d.bitsPerWeight.toFixed(2)} bpw`].filter(Boolean).join(' · ')}
          </Item>
        )}
        {d.contextLength != null && <Item label="Trained context">{tokens(d.contextLength)}</Item>}
        {(d.hfRepoId || d.revision || d.downloadDate) && (
          <Item label="Source">
            {d.hfRepoId && repo && (
              <Button variant="link" size="sm" className="font-mono text-xs font-normal" rightIcon={<Icon icon={ExternalLink} size={12} />} onClick={() => openUrl(repo)}>
                {d.hfRepoId}
              </Button>
            )}
            <span className="block text-text-secondary">
              {/* A sha short, a ref whole: the daemon shortens, this shows. */}
              {d.revision && (
                <span className="font-mono" title={d.revision.value}>
                  {d.revision.short}
                </span>
              )}
              {d.revision && d.downloadDate && ' · '}
              {d.downloadDate && <When at={d.downloadDate} before="downloaded " />}
            </span>
          </Item>
        )}
        <Item label="From a terminal">
          <span className="flex items-start gap-xs">
            <code className="min-w-0 flex-1 break-all font-mono">{command}</code>
            <CopyValue value={command} label="Copy the command" onCopied={() => showToast('Copied the command', 'success')} />
          </span>
        </Item>
      </dl>
    </details>
  );
};
