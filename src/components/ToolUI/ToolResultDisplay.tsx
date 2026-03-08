import React from 'react';
import { Check, ChevronDown, ChevronRight, Clipboard } from 'lucide-react';
import type { ToolResultRenderer } from '../../services/tools/types';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';
import { getToolRegistry } from '../../services/tools/registry';
import { fallbackRenderer } from '../../services/tools/renderers';

export interface ToolResultDisplayProps {
  toolName: string;
  result: unknown;
}

function safeRenderSummary(renderer: ToolResultRenderer, result: unknown, toolName: string): string {
  try {
    return renderer.renderSummary?.(result, toolName) ?? fallbackRenderer.renderSummary!(result, toolName);
  } catch {
    try {
      return fallbackRenderer.renderSummary!(result, toolName);
    } catch {
      return toolName;
    }
  }
}

/**
 * Collapsible card for displaying a single tool result.
 * Dispatches to the registered ToolResultRenderer for the tool,
 * falling back to the generic JSON renderer if none is registered or
 * if the renderer throws.
 *
 * This is the single source of truth for result rendering across
 * GenericToolUI (inline chat bubble) and ToolDetailsModal.
 */
export const ToolResultDisplay: React.FC<ToolResultDisplayProps> = ({ toolName, result }) => {
  const [isExpanded, setIsExpanded] = React.useState(false);
  const [copied, setCopied] = React.useState(false);

  const renderer = getToolRegistry().getRenderer(toolName) ?? fallbackRenderer;

  const summary = React.useMemo(
    () => safeRenderSummary(renderer, result, toolName),
    [renderer, result, toolName],
  );

  const body = React.useMemo(() => {
    try {
      return renderer.renderResult(result, toolName);
    } catch {
      return fallbackRenderer.renderResult(result, toolName);
    }
  }, [renderer, result, toolName]);

  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation();
    const text = (() => {
      try {
        return JSON.stringify(result, null, 2) ?? String(result);
      } catch {
        return String(result);
      }
    })();
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div className="border border-border rounded-lg bg-background overflow-hidden my-2 text-[13px]">
      {/* Header — always visible, toggles body */}
      <button
        type="button"
        className="flex items-center gap-2 w-full px-3 py-2 text-left cursor-pointer bg-transparent hover:bg-background-secondary transition-colors duration-150"
        onClick={() => setIsExpanded((prev) => !prev)}
        aria-expanded={isExpanded}
      >
        <span className="shrink-0 text-text-secondary" aria-hidden>
          <Icon icon={isExpanded ? ChevronDown : ChevronRight} size={14} />
        </span>

        <span className="flex-1 truncate font-mono text-[11px] text-text-secondary">
          {summary}
        </span>

        {/* Copy button — stopPropagation prevents accordion toggle */}
        <span
          role="button"
          tabIndex={0}
          aria-label="Copy result as JSON"
          className={cn(
            'shrink-0 inline-flex items-center justify-center w-6 h-6 rounded transition-colors duration-150',
            'text-text-muted hover:text-text hover:bg-background-tertiary',
            copied && 'text-[#4ade80]',
          )}
          onClick={handleCopy}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              handleCopy(e as unknown as React.MouseEvent);
            }
          }}
        >
          <Icon icon={copied ? Check : Clipboard} size={13} />
        </span>
      </button>

      {/* Body — visible when expanded */}
      {isExpanded && (
        <div className="px-3 py-2 border-t border-border overflow-x-auto max-h-[400px] overflow-y-auto">
          {body}
        </div>
      )}
    </div>
  );
};

export default ToolResultDisplay;
