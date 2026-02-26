/**
 * ToolSupportIndicator
 *
 * Small pill badge shown in the chat header to communicate whether the active
 * model supports tool/function calling.
 *
 * Visibility rules (matching issue #256 spec):
 *   - No tools configured  → render nothing
 *   - supports = null       → render nothing (unknown / still loading)
 *   - supports = true       → "⚡ Tools active"  (green pill)
 *   - supports = false      → "💬 Chat only"     (amber pill, tooltip explains why)
 */

import { Zap, MessageCircle } from 'lucide-react';
import { Icon } from './ui/Icon';
import { cn } from '../utils/cn';

export interface ToolSupportIndicatorProps {
  /** Whether the model supports tool calls. Null means unknown — renders nothing. */
  supports: boolean | null;
  /** Whether any tools are currently enabled. When false the indicator is hidden. */
  hasToolsConfigured: boolean;
  /** Detected tool-calling format (e.g. "hermes", "llama3"). Used in hover tooltip. */
  toolFormat?: string | null;
  className?: string;
}

export function ToolSupportIndicator({
  supports,
  hasToolsConfigured,
  toolFormat,
  className,
}: ToolSupportIndicatorProps) {
  // Only show when tools are in use and we have a definitive answer
  if (!hasToolsConfigured || supports === null) return null;

  if (supports) {
    return (
      <span
        className={cn(
          'inline-flex items-center gap-1 py-[2px] px-2 text-[11px] font-medium rounded-[10px] shrink-0',
          'bg-[#10b981]/15 text-[#10b981]',
          className,
        )}
        title={
          toolFormat
            ? `Tools active — model supports tool calling (${toolFormat} format)`
            : 'Tools active — model supports tool calling'
        }
      >
        <Icon icon={Zap} size={11} />
        Tools active
      </span>
    );
  }

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 py-[2px] px-2 text-[11px] font-medium rounded-[10px] shrink-0',
        'bg-[#f59e0b]/15 text-[#f59e0b]',
        className,
      )}
      title="This model does not support tool/function calling — tools are disabled for this chat"
    >
      <Icon icon={MessageCircle} size={11} />
      Chat only
    </span>
  );
}
