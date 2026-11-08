import { FC, useId, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { StatusMark } from '../../status/StatusMark';
import { ToolResultDisplay, resultText } from '../../ToolUI';
import { ToolRunProgress, type ToolCallRow as Row } from '../../ToolExecutionProgress';
import type { PreviewFrame } from '../../../hooks/useGglibRuntime/runPreviews';
import { secondsWords } from '../../../readings/turnFigures';
import { cn } from '../../../utils/cn';
import { useCopied } from '../../CopyValue';

const LABEL = 'text-xs font-semibold text-text-muted';
/** A 12 px text button: no box. */
const TEXT_BUTTON = 'self-start h-auto p-0 text-xs font-normal hover:bg-transparent';

/**
 * What the row says after the tool's name: that it runs (how far it has
 * got is said under the line, where the tool says it), how long it took, or
 * why it failed.
 */
function stateWords(row: Row): string | null {
  if (row.state === 'running') return row.progress ? null : 'running';
  if (row.state === 'failed') return row.error ? `failed · ${row.error}` : 'failed';
  return row.durationMs !== undefined ? secondsWords(row.durationMs / 1000) : null;
}

/** Copy `value` as JSON, then "Copied" for a second, as every copy says it (`useCopied`, its timer cleared on unmount). */
const CopyJson: FC<{ value: unknown; name: string }> = ({ value, name }) => {
  const [copied, showCopied] = useCopied();
  return (
    <Button
      variant="ghost"
      size="sm"
      className={TEXT_BUTTON}
      aria-label={name}
      onClick={() => {
        navigator.clipboard.writeText(resultText(value)).then(showCopied).catch(() => {});
      }}
    >
      {copied ? 'Copied' : 'Copy'}
    </Button>
  );
};

/**
 * One tool call in a reply's fold: its name, the MCP server it belongs to,
 * and its state (how far it has got while it runs, its time once done, why
 * it failed), opening in place onto the arguments and their Copy (while it
 * runs too), then the result, drawn by the tool's renderer or the generic
 * one (JSON, Markdown or a sortable table), then its Copy. A tool still
 * running shows its progress and the picture it is making under the line,
 * open or not; the images a finished tool made are the reply's, not here.
 */
export const ToolCallRow: FC<{ row: Row; preview?: PreviewFrame }> = ({ row, preview }) => {
  const [open, setOpen] = useState(false);
  const detailId = useId();
  const words = stateWords(row);
  return (
    <li className="flex flex-col gap-xs">
      <Button
        variant="ghost"
        size="sm"
        className={cn(TEXT_BUTTON, 'gap-xs max-w-full tabular-nums')}
        aria-expanded={open}
        aria-controls={detailId}
        leftIcon={<Icon icon={ChevronRight} size={12} className={cn('transition-transform duration-150', open && 'rotate-90')} />}
        onClick={() => setOpen(!open)}
      >
        {row.state === 'running' && <StatusMark kind="changing" label="Running" className="text-xs" />}
        {row.state === 'failed' && <StatusMark kind="failed" label="Failed" className="text-xs" />}
        <span className="font-semibold text-text">{row.label}</span>
        {row.server && <span>on {row.server}</span>}
        {words && <span className={cn('truncate', row.state === 'failed' && 'text-danger')}>{words}</span>}
      </Button>
      {row.state === 'running' && <ToolRunProgress progress={row.progress} preview={preview} />}
      <div id={detailId} hidden={!open} className="pl-base flex flex-col gap-xs">
        <div className="flex items-baseline gap-sm">
          <span className={LABEL}>
            Arguments <span className="font-mono font-normal">{row.toolName}</span>
          </span>
          <CopyJson value={row.args ?? {}} name="Copy arguments" />
        </div>
        <pre className="m-0 p-sm bg-surface rounded-base font-mono text-xs text-text overflow-x-auto whitespace-pre max-h-[300px]">
          {resultText(row.args ?? {})}
        </pre>
        {row.state !== 'running' && (
          <>
            <span className={LABEL}>Result</span>
            <ToolResultDisplay toolName={row.toolName} result={row.result} />
            <CopyJson value={row.result} name="Copy result" />
          </>
        )}
      </div>
    </li>
  );
};
