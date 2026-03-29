import React, { useState } from 'react';
import { useMessage } from '@assistant-ui/react';
import type { ThreadMessage } from '@assistant-ui/react';
import ToolDetailsModal from './ToolDetailsModal';
import { Wrench } from 'lucide-react';
import { cn } from '../../utils/cn';
import { Icon } from '../ui/Icon';

type ToolCallPart = Extract<ThreadMessage['content'][number], { type: 'tool-call' }>;

/**
 * Badge showing tools used in a message.
 * Displays tool names and status. Click to open details modal.
 */
const ToolUsageBadge: React.FC = () => {
  const message = useMessage();
  const [isModalOpen, setIsModalOpen] = useState(false);

  // Extract tool call parts from message content
  const toolCalls = message.content.filter(
    (part): part is ToolCallPart => 
      typeof part !== 'string' && part.type === 'tool-call'
  );

  // Don't render if no tools were used
  if (toolCalls.length === 0) {
    return null;
  }

  // Determine badge status based on tool call results
  const getToolStatus = (): 'running' | 'success' | 'error' | 'mixed' => {
    let hasSuccess = false;
    let hasError = false;

    for (const call of toolCalls) {
      if (!('result' in call)) {
        // At least one tool has no result yet — batch is still in flight.
        return 'running';
      }
      const result = call.result as any;
      if (result && typeof result === 'object') {
        if ('error' in result || result.success === false) {
          hasError = true;
        } else {
          hasSuccess = true;
        }
      } else {
        hasSuccess = true;
      }
    }

    if (hasError && hasSuccess) return 'mixed';
    if (hasError) return 'error';
    return 'success';
  };

  const status = getToolStatus();

  // Get tool names, truncate if more than 2
  const toolNames = toolCalls.map((call) => call.toolName);
  const displayNames =
    toolNames.length <= 2
      ? toolNames.join(', ')
      : `${toolNames.slice(0, 2).join(', ')} & ${toolNames.length - 2} more`;

  const runningCount = toolCalls.filter(c => !('result' in c)).length;
  const ariaLabel =
    status === 'running'
      ? `${runningCount} of ${toolCalls.length} tool${toolCalls.length === 1 ? '' : 's'} running`
      : `${toolCalls.length} tool${toolCalls.length === 1 ? '' : 's'} ${status}`;

  return (
    <>
      <button
        className={cn(
          'inline-flex items-center gap-1 py-[2px] px-2 text-[11px] font-medium border-none rounded-[10px] cursor-pointer transition-all duration-150 ml-2 hover:scale-105 hover:shadow-[0_2px_4px_rgba(0,0,0,0.1)] active:scale-[0.98]',
          status === 'running' && 'bg-primary-subtle text-primary border border-primary-border hover:bg-primary/20 animate-pulse',
          status === 'success' && 'bg-success-subtle text-success border border-success-border hover:bg-success/20',
          status === 'error' && 'bg-danger-subtle text-danger border border-danger-border hover:bg-danger/20',
          status === 'mixed' && 'bg-warning-subtle text-warning border border-warning-border hover:bg-warning/20',
        )}
        onClick={() => setIsModalOpen(true)}
        title="Click to view tool execution details"
        aria-live="polite"
        aria-label={ariaLabel}
      >
        <span className="text-[12px] leading-none" aria-hidden="true">
          <Icon icon={Wrench} size={14} />
        </span>
        <span className="leading-none whitespace-nowrap overflow-hidden text-ellipsis max-w-[200px]">{displayNames}</span>
      </button>

      {isModalOpen && (
        <ToolDetailsModal
          toolCalls={toolCalls}
          onClose={() => setIsModalOpen(false)}
        />
      )}
    </>
  );
};

export default ToolUsageBadge;
