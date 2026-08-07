import React, { useState } from 'react';
import { useMessage } from '@assistant-ui/react';
import type { ThreadMessage } from '@assistant-ui/react';
import ToolDetailsModal from './ToolDetailsModal';
import { Wrench } from 'lucide-react';
import { cn } from '../../utils/cn';
import { Icon } from '../ui/Icon';
import { Chip } from '../ui/Chip';

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
      <Chip
        size="sm"
        variant={
          status === 'running' ? 'primary'
          : status === 'success' ? 'success'
          : status === 'error' ? 'danger'
          : 'warning'
        }
        className={cn('ml-2 rounded-full max-w-[220px]', status === 'running' && 'animate-pulse')}
        leftIcon={<Icon icon={Wrench} size={12} />}
        onClick={() => setIsModalOpen(true)}
        title={`Click to view tool execution details. ${ariaLabel}`}
      >
        {displayNames}
      </Chip>

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
