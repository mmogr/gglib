import { FC } from 'react';
import { AlertTriangle, CheckCircle2, HelpCircle, XCircle } from 'lucide-react';
import { HfQuantization, FitStatus } from '../../types';
import { formatBytes } from '../../utils/format';
import { useSystemMemory } from '../../hooks/useSystemMemory';
import { useSettings } from '../../hooks/useSettings';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';

// Fit indicator component
interface FitIndicatorProps {
  sizeBytes: number;
  checkFit: (sizeBytes: number) => FitStatus;
  getTooltip: (sizeBytes: number) => string;
}

const FitIndicator: FC<FitIndicatorProps> = ({ sizeBytes, checkFit, getTooltip }) => {
  const status = checkFit(sizeBytes);
  const tooltip = getTooltip(sizeBytes);

  const iconMap: Record<FitStatus, { icon: typeof CheckCircle2; className: string }> = {
    fits: { icon: CheckCircle2, className: '' },
    tight: { icon: AlertTriangle, className: '' },
    wont_fit: { icon: XCircle, className: '' },
    unknown: { icon: HelpCircle, className: 'grayscale opacity-60' },
  };

  const { icon, className } = iconMap[status];

  return (
    <span 
      className={cn('text-base cursor-help', className)}
      title={tooltip}
      aria-label={tooltip}
    >
      <Icon icon={icon} size={14} />
    </span>
  );
};

interface QuantizationTableProps {
  /** The repository's quantizations, in the order shown */
  quantizations: HfQuantization[];
  /** Name of the selected quantization */
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
 * The quantization table: one row per quantization with its weights' size,
 * shard count, memory fit and download button. One row is the selected one.
 * The size and shard count are the weights alone; the fit counts the
 * projector fetched with them too, since both are loaded.
 */
export const QuantizationTable: FC<QuantizationTableProps> = ({
  quantizations,
  selectedName,
  onSelect,
  onDownload,
  downloadsDisabled,
  disabledReason,
}) => {
  // Memory fit checking
  const { checkFit, getTooltip, loading: memoryLoading } = useSystemMemory();
  const { settings } = useSettings();
  const showFitIndicators = settings?.showMemoryFitIndicators ?? true;

  return (
    <div className="flex flex-col border border-border rounded-lg overflow-hidden bg-surface">
      <div className="grid grid-cols-[1fr_80px_60px_50px_90px] gap-sm px-base py-md bg-surface-elevated text-sm font-semibold text-text">
        <span>Quant</span>
        <span>Size</span>
        <span>Shards</span>
        {showFitIndicators && !memoryLoading && (
          <span>Fit</span>
        )}
        <span></span>
      </div>
      <div className="flex flex-col max-h-[300px] overflow-y-auto">
        {quantizations.map((quant) => (
          <div
            key={quant.name}
            data-testid={`quant-row-${quant.name}`}
            aria-current={quant.name === selectedName ? 'true' : undefined}
            onClick={() => onSelect(quant)}
            onFocus={() => onSelect(quant)}
            className={cn(
              'grid grid-cols-[1fr_80px_60px_50px_90px] gap-sm px-base py-md items-center border-b border-border-light last:border-b-0 cursor-pointer transition-colors duration-150 ease-linear hover:bg-surface-hover',
              quant.name === selectedName && 'bg-surface-hover',
            )}
          >
            <span className="overflow-hidden text-ellipsis whitespace-nowrap">
              <span className="font-medium text-text">{quant.name}</span>
            </span>
            <span className="text-sm text-text-secondary text-right">{formatBytes(quant.size_bytes)}</span>
            <span className="text-sm text-text-secondary text-center">
              {quant.is_sharded ? quant.shard_count : 1}
            </span>
            {showFitIndicators && !memoryLoading && (
              <span className="text-center">
                <FitIndicator
                  sizeBytes={quant.size_bytes + (quant.projector?.size_bytes ?? 0)}
                  checkFit={checkFit}
                  getTooltip={getTooltip}
                />
              </span>
            )}
            <span className="text-right">
              <Button
                size="sm"
                onClick={() => onDownload(quant)}
                disabled={downloadsDisabled}
                title={downloadsDisabled ? disabledReason : `Download ${quant.name}`}
              >
                Download
              </Button>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
};
