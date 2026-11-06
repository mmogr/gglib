import { FC } from 'react';
import { AlertTriangle, CheckCircle2, HelpCircle, XCircle } from 'lucide-react';
import { HfQuantization, VariantFit } from '../../types';
import { formatBytes } from '../../utils/format';
import { useSettings } from '../../hooks/useSettings';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';

/**
 * The daemon's verdict on a variant, drawn: a shape per kind, and the
 * daemon's own words as the title. Nothing is judged here.
 */
const FitIndicator: FC<{ fit: VariantFit; word: string }> = ({ fit, word }) => {
  const iconMap: Record<VariantFit['kind'], { icon: typeof CheckCircle2; className: string }> = {
    fits: { icon: CheckCircle2, className: '' },
    tight: { icon: AlertTriangle, className: '' },
    too_big: { icon: XCircle, className: '' },
    unknown: { icon: HelpCircle, className: 'grayscale opacity-60' },
  };
  const { icon, className } = iconMap[fit.kind];

  return (
    <span className={cn('text-base cursor-help', className)} title={word} aria-label={word}>
      <Icon icon={icon} size={14} />
    </span>
  );
};

/** Why a draft or helper variant offers no download. */
const HELPER_FILES_REFUSAL = "gglib can't run draft or helper files on their own";

interface QuantizationTableProps {
  /** The repository's variants, in the order shown */
  quantizations: HfQuantization[];
  /** Label of the selected variant */
  selectedName: string | undefined;
  /** Called when a row is clicked, or focus enters it */
  onSelect: (quant: HfQuantization) => void;
  /** Called when a row's download button is pressed */
  onDownload: (quant: HfQuantization) => void;
  /** Whether download buttons should be disabled (queue full) */
  downloadsDisabled: boolean;
  /** Tooltip text when downloads are disabled */
  disabledReason?: string;
}

/**
 * The variant table: one row per variant, by its uploader's label, with its
 * size, shard count, the daemon's fit verdict and download button. One row
 * is the selected one. The size, the shard count and the daemon's fit are
 * the weights alone, as the download's own pick judges them; the projector
 * fetched beside them counts for neither. A draft or helper is marked as
 * one, and its download is disabled.
 */
export const QuantizationTable: FC<QuantizationTableProps> = ({
  quantizations,
  selectedName,
  onSelect,
  onDownload,
  downloadsDisabled,
  disabledReason,
}) => {
  const { settings } = useSettings();
  const showFitIndicators = settings?.showMemoryFitIndicators ?? true;

  return (
    <div className="flex flex-col border border-border rounded-lg overflow-hidden bg-surface">
      <div className="grid grid-cols-[1fr_80px_60px_50px_90px] gap-sm px-base py-md bg-surface-elevated text-sm font-semibold text-text">
        <span>Quant</span>
        <span>Size</span>
        <span>Shards</span>
        {showFitIndicators && <span>Fit</span>}
        <span></span>
      </div>
      <div className="flex flex-col max-h-[300px] overflow-y-auto">
        {quantizations.map((quant) => {
          const helper = quant.role === 'Sidecar';
          return (
          <div
            key={quant.label}
            data-testid={`quant-row-${quant.label}`}
            aria-current={quant.label === selectedName ? 'true' : undefined}
            onClick={() => onSelect(quant)}
            onFocus={() => onSelect(quant)}
            className={cn(
              'grid grid-cols-[1fr_80px_60px_50px_90px] gap-sm px-base py-md items-center border-b border-border-light last:border-b-0 cursor-pointer transition-colors duration-150 ease-linear hover:bg-surface-hover',
              quant.label === selectedName && 'bg-surface-hover',
            )}
          >
            <span className="overflow-hidden text-ellipsis whitespace-nowrap">
              <span className="font-medium text-text">{quant.label}</span>
              {helper && (
                <span className="ml-sm text-xs text-text-secondary" title={HELPER_FILES_REFUSAL}>
                  draft or helper
                </span>
              )}
            </span>
            <span className="text-sm text-text-secondary text-right">{formatBytes(quant.size_bytes)}</span>
            <span className="text-sm text-text-secondary text-center">
              {quant.is_sharded ? quant.shard_count : 1}
            </span>
            {showFitIndicators && (
              <span className="text-center">
                {quant.fit && quant.fit_word && (
                  <FitIndicator fit={quant.fit} word={quant.fit_word} />
                )}
              </span>
            )}
            <span className="text-right">
              <Button
                size="sm"
                onClick={() => onDownload(quant)}
                disabled={downloadsDisabled || helper}
                title={
                  helper
                    ? HELPER_FILES_REFUSAL
                    : downloadsDisabled
                      ? disabledReason
                      : `Download ${quant.label}`
                }
              >
                Download
              </Button>
            </span>
          </div>
          );
        })}
      </div>
    </div>
  );
};
