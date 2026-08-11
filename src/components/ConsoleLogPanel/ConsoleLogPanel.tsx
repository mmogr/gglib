import { FC, useRef, useEffect, useCallback } from 'react';
import Anser from 'anser';
import { ClipboardCopy, Pause, Play, Trash2, Monitor } from 'lucide-react';
import { useServerLogs, ServerLogEntry } from '../../hooks/useServerLogs';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { EmptyState } from '../primitives';
import './ConsoleLogPanel.css';

interface ConsoleLogPanelProps {
  serverPort: number;
}

/**
 * Renders a single log line with ANSI color support
 */
const LogLine: FC<{ entry: ServerLogEntry }> = ({ entry }) => {
  // Parse ANSI codes and convert to HTML
  const html = Anser.ansiToHtml(Anser.escapeForHtml(entry.line), {
    use_classes: true,
  });

  return (
    <div 
      className="console-log-line"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
};

/**
 * Terminal-style log viewer panel for llama-server output.
 * Features auto-scroll, ANSI color support, and copy/clear controls.
 */
const ConsoleLogPanel: FC<ConsoleLogPanelProps> = ({ serverPort }) => {
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

  const handleToggleAutoScroll = useCallback(() => {
    setIsAutoScroll(!isAutoScroll);
    isUserScrollingRef.current = false;
    
    // If enabling auto-scroll, jump to bottom
    if (!isAutoScroll && scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight;
    }
  }, [isAutoScroll, setIsAutoScroll]);

  return (
    <div className="flex flex-col overflow-y-auto overflow-x-hidden relative flex-1 bg-surface md:h-full md:min-h-0">
      <div className="p-md border-b border-border-light shrink-0">
        <div className="flex items-center justify-between gap-md">
          <h3 className="m-0 text-lg font-semibold text-text">Server Output</h3>
          <div className="flex gap-xs">
            <Button
              variant="secondary"
              size="sm"
              onClick={handleToggleAutoScroll}
              title={isAutoScroll ? 'Following new output' : 'Auto-scroll paused'}
              leftIcon={<Icon icon={isAutoScroll ? Play : Pause} size={14} />}
            >
              {isAutoScroll ? 'Following' : 'Paused'}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={copyAllLogs}
              title="Copy all logs to clipboard"
              leftIcon={<Icon icon={ClipboardCopy} size={14} />}
            >
              Copy
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={clearLogs}
              title="Clear log display"
              leftIcon={<Icon icon={Trash2} size={14} />}
            >
              Clear
            </Button>
          </div>
        </div>
      </div>

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
          <div className="whitespace-pre-wrap break-all">
            {logs.map((entry, index) => (
              <LogLine key={`${entry.timestamp}-${index}`} entry={entry} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default ConsoleLogPanel;
