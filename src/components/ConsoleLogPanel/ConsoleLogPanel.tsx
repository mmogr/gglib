import { FC, useRef, useEffect, useCallback } from 'react';
import Anser from 'anser';
import { ClipboardCopy, Pause, Play, Trash2, Monitor } from 'lucide-react';
import { useServerLogs, ServerLogEntry } from '../../hooks/useServerLogs';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
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
    <div className="flex flex-col h-full min-h-0 overflow-y-auto overflow-x-hidden relative flex-1 max-md:h-auto max-md:max-h-none console-log-panel">
      <div className="p-base border-b border-border bg-background shrink-0">
        <div className="console-log-header">
          <h3 className="console-log-title">Server Output</h3>
          <div className="console-log-controls">
            <Button
              variant={isAutoScroll ? 'primary' : 'secondary'}
              size="sm"
              onClick={handleToggleAutoScroll}
              title={isAutoScroll ? 'Auto-scroll enabled' : 'Auto-scroll disabled'}
              leftIcon={<Icon icon={isAutoScroll ? Play : Pause} size={14} />}
            >
              {isAutoScroll ? 'Auto' : 'Paused'}
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
        className="console-log-content"
        onScroll={handleScroll}
      >
        {logs.length === 0 ? (
          <div className="console-log-empty">
            <span className="console-log-empty-icon" aria-hidden>
              <Icon icon={Monitor} size={28} />
            </span>
            <p>Waiting for server output...</p>
            <p className="console-log-empty-hint">
              Logs will appear here as the server processes requests
            </p>
          </div>
        ) : (
          <div className="console-log-lines">
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
