import { FC, useRef, useEffect, useCallback, type ReactNode } from 'react';
import Anser from 'anser';
import { ChevronLeft, Copy, Monitor, MoreHorizontal, X } from 'lucide-react';
import { useServerLogs, ServerLogEntry } from '../../hooks/useServerLogs';
import { Icon } from '../../components/ui/Icon';
import { Button } from '../../components/ui/Button';
import { EmptyState } from '../../components/primitives';
import { Menu } from '../../places/library/Menu';
import { cn } from '../../utils/cn';
import './ConsoleLogPanel.css';

/** A time since the first line shown, as `m:ss.mmm`. */
export function sinceFirst(ms: number): string {
  const t = Math.max(0, Math.round(ms));
  const seconds = Math.floor((t % 60_000) / 1000);
  return `${Math.floor(t / 60_000)}:${String(seconds).padStart(2, '0')}.${String(t % 1000).padStart(3, '0')}`;
}

/**
 * Renders a single log line with ANSI color support, after its time since
 * the first line shown.
 */
const LogLine: FC<{ entry: ServerLogEntry; first: number }> = ({ entry, first }) => {
  // Parse ANSI codes and convert to HTML
  const html = Anser.ansiToHtml(Anser.escapeForHtml(entry.line), {
    use_classes: true,
  });

  return (
    <div className="console-log-line flex gap-sm">
      <span className="shrink-0 select-none text-text-muted tabular-nums">{sinceFirst(entry.timestamp - first)}</span>
      <span className="min-w-0" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
};

interface LogHeaderProps {
  modelName: string;
  onBack: () => void;
  onClose: () => void;
  /** Following and Paused, ⧉ and ⋯, while there is a log to act on. */
  controls?: ReactNode;
}

/** `‹ <name> · Log`, the log's controls, and ×. */
export const LogHeader: FC<LogHeaderProps> = ({ modelName, onBack, onClose, controls }) => (
  <div className="flex shrink-0 items-center gap-xs p-md">
    <Button variant="ghost" size="sm" iconOnly aria-label={`Back to ${modelName}`} title={`Back to ${modelName}`} onClick={onBack}>
      <Icon icon={ChevronLeft} size={14} />
    </Button>
    <h2 className="m-0 min-w-0 truncate text-base font-semibold text-text">{`${modelName} · Log`}</h2>
    <span className="flex-1" />
    {controls}
    <Button variant="ghost" size="sm" iconOnly aria-label="Close" title="Close" onClick={onClose}>
      <Icon icon={X} size={14} />
    </Button>
  </div>
);

interface ConsoleLogPanelProps extends Omit<LogHeaderProps, 'controls'> {
  serverPort: number;
}

/**
 * The live llama-server output of one port, with ANSI color support. It
 * follows new output until paused (by the switch, or by scrolling away from
 * the end); Following again jumps to the end. ⧉ copies every line, and
 * ⋯ Clear view empties the view (the daemon keeps the log).
 */
const ConsoleLogPanel: FC<ConsoleLogPanelProps> = ({ serverPort, ...header }) => {
  const { logs, clearLogs, isAutoScroll, setIsAutoScroll, copyAllLogs } = useServerLogs({
    serverPort,
  });

  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const isUserScrollingRef = useRef(false);

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    if (isAutoScroll && scrollContainerRef.current && !isUserScrollingRef.current) {
      scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight;
    }
  }, [logs, isAutoScroll]);

  // Detect user scroll to disable auto-scroll temporarily
  const handleScroll = useCallback(() => {
    if (!scrollContainerRef.current) return;

    const { scrollTop, scrollHeight, clientHeight } = scrollContainerRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;

    // If user scrolled away from bottom, pause auto-scroll
    if (!isAtBottom && isAutoScroll) {
      isUserScrollingRef.current = true;
    } else if (isAtBottom) {
      isUserScrollingRef.current = false;
    }
  }, [isAutoScroll]);

  const follow = useCallback((on: boolean) => {
    setIsAutoScroll(on);
    isUserScrollingRef.current = false;

    // If enabling auto-scroll, jump to bottom
    if (on && scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight;
    }
  }, [setIsAutoScroll]);

  // A word switch: the state in force is the word drawn in ink.
  const word = (on: boolean, label: string) => (
    <Button
      variant="ghost"
      size="sm"
      aria-pressed={isAutoScroll === on}
      className={cn('px-xs font-normal', isAutoScroll === on ? 'text-text' : 'text-text-muted')}
      onClick={() => follow(on)}
    >
      {label}
    </Button>
  );

  const controls = (
    <>
      <span role="group" aria-label="Scrolling" className="flex items-center">
        {word(true, 'Following')}
        <span aria-hidden className="text-xs text-text-muted">·</span>
        {word(false, 'Paused')}
      </span>
      <Button variant="ghost" size="sm" iconOnly aria-label="Copy the log" title="Copy the log" onClick={copyAllLogs}>
        <Icon icon={Copy} size={14} />
      </Button>
      <Menu iconOnly name="More" label={<Icon icon={MoreHorizontal} size={14} />} items={[{ label: 'Clear view', onPick: clearLogs }]} />
    </>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <LogHeader {...header} controls={controls} />

      <div
        ref={scrollContainerRef}
        tabIndex={0}
        role="log"
        aria-label="Server output"
        className="console-log-content flex-1 overflow-y-auto overflow-x-auto bg-terminal font-mono text-xs leading-[1.5] p-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary"
        onScroll={handleScroll}
      >
        {logs.length === 0 ? (
          <EmptyState
            className="h-full font-sans"
            icon={<Icon icon={Monitor} size={24} />}
            title="Waiting for server output"
            description="Logs appear here once the server starts handling requests"
          />
        ) : (
          // Lines wrap at spaces only; a token longer than the width scrolls.
          <div className="whitespace-pre-wrap">
            {logs.map((entry, index) => (
              <LogLine key={`${entry.timestamp}-${index}`} entry={entry} first={logs[0].timestamp} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default ConsoleLogPanel;
